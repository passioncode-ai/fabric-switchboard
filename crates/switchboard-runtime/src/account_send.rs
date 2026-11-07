//! SB-283: move an account's sign-in to another machine over ssh, without a new sign-in there.
//!
//! A sign-in has one holder: a renewal on one machine ends the other's
//! (docs/research/two-machines-2026-10-23.md, D-2). So this machine marks the account as used
//! elsewhere first (it never renews it again, SB-293), then hands the provider's tokens to the
//! other machine's `switchboard accounts add` on stdin, checks there that the provider answers,
//! and undoes both on any failure. Raised by the operator 2026-11-07 («поднимай», relayed by
//! studio-agent, board thread `f32a24c0`).

use serde_json::{json, Value};
use switchboard_core::{AuthKind, Provider, Store};

/// The macOS app's own command, the default program on the other machine.
pub const REMOTE_DEFAULT: &str = "/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard";
pub const TARGET_INVALID: &str =
    "Name the other machine as ssh knows it: letters, digits, dots, hyphens, @ and : only.";
pub const REMOTE_INVALID: &str =
    "Give the other machine's switchboard as an absolute path without quotes.";
pub const NOT_A_SIGN_IN: &str =
    "Only a Claude or Codex subscription sign-in moves this way. Give an API key through your vault.";
pub const NOT_HERE: &str = "This account is already used elsewhere or turned off here.";
pub const SELECTED: &str = "Select another account for its group before moving this one.";
pub const ADD_FAILED: &str =
    "The other machine did not save the sign-in. Nothing changed here; check ssh and its Switchboard.";
pub const NOT_VERIFIED: &str = "The other machine saved the sign-in but the provider did not answer it there. It was removed there and stays here.";

/// Runs one command on the other machine: `ssh <target> -- <remote command>`, with `stdin` when
/// given; answers its standard output, or an error when it failed.
pub type Run<'a> = dyn Fn(&[String], Option<&[u8]>) -> Result<String, String> + 'a;

fn target_valid(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 255
        && !target.starts_with('-')
        && target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'@' | b':'))
}
/// One word for the remote shell: single-quoted, a quote inside written `'\''`.
fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}
fn remote(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .map(quoted)
        .collect::<Vec<_>>()
        .join(" ")
}
fn data(output: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(output.lines().last()?.trim()).ok()?;
    (value["ok"] == true).then(|| value["data"].clone())
}

pub fn send(
    store: &Store,
    id: &str,
    target: &str,
    pool: &str,
    program: &str,
    now: i64,
    run: &Run<'_>,
) -> Result<Value, String> {
    if !target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    if !program.starts_with('/') || program.contains(['\'', '"', '\n']) {
        return Err(REMOTE_INVALID.into());
    }
    let snapshot = store.snapshot()?;
    let account = snapshot
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or("Account not found")?;
    if account.kind != AuthKind::OAuth {
        return Err(NOT_A_SIGN_IN.into());
    }
    if !account.enabled || snapshot.elsewhere.contains_key(id) {
        return Err(NOT_HERE.into());
    }
    if snapshot.routes.values().any(|v| v == id) {
        return Err(SELECTED.into());
    }
    let credential = store.stored_credential(id)?;
    let provider = match account.provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    };
    let ssh = |command: String| vec![target.to_owned(), "--".to_owned(), command];
    // From here this machine never renews it: the tokens leave with one holder.
    store.mark_elsewhere_at(id, true, now, Some(target))?;
    let undo_here = || store.mark_elsewhere(id, false, now);
    let added = run(
        &ssh(remote(
            program,
            &[
                "--json",
                "accounts",
                "add",
                "--provider",
                provider,
                "--kind",
                "oauth",
                "--label",
                &account.label,
                "--pool",
                pool,
                "--secret-stdin",
            ],
        )),
        Some(credential.to_oauth_json(account.provider).as_bytes()),
    )
    .ok()
    .and_then(|out| data(&out))
    .and_then(|d| d["id"].as_str().map(str::to_owned));
    let Some(remote_id) = added else {
        undo_here()?;
        return Err(ADD_FAILED.into());
    };
    let verified = run(
        &ssh(remote(program, &["--json", "usage", &remote_id])),
        None,
    )
    .ok()
    .and_then(|out| data(&out))
    .is_some();
    if !verified {
        let _ = run(
            &ssh(remote(
                program,
                &["--json", "accounts", "remove", &remote_id],
            )),
            None,
        );
        undo_here()?;
        return Err(NOT_VERIFIED.into());
    }
    Ok(json!({"moved": true, "target": target, "remote_id": remote_id, "marked_elsewhere": true}))
}

/// The real runner: the system's `ssh` in batch mode (no prompt), 2 minutes at most.
pub fn ssh_runner(args: &[String], stdin: Option<&[u8]>) -> Result<String, String> {
    use std::io::Write;
    let program = crate::launch::find_program("ssh").ok_or("ssh is not installed here.")?;
    let mut child = std::process::Command::new(program)
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=15"])
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "ssh could not start.".to_string())?;
    if let Some(mut input) = child.stdin.take() {
        if let Some(bytes) = stdin {
            let _ = input.write_all(bytes);
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "ssh failed.".to_string())? {
            let mut out = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                use std::io::Read;
                let _ = stdout.take(1 << 20).read_to_string(&mut out);
            }
            return if status.success() {
                Ok(out)
            } else {
                Err("ssh failed.".into())
            };
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("ssh timed out.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use switchboard_core::{Credential, MemoryVault};

    fn store_with(kind: AuthKind) -> (tempfile::TempDir, Store, String) {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let credential = if kind == AuthKind::OAuth {
            Credential {
                access_token: "synthetic-access".into(),
                refresh_token: Some("synthetic-refresh".into()),
                id_token: None,
                native_context: None,
                expires_at: Some(2_000_000_000),
                account_id: Some("synthetic-account".into()),
            }
        } else {
            Credential::parse(Provider::Claude, kind, "synthetic").unwrap()
        };
        let account = store
            .add(
                "Work O'Neil".into(),
                Provider::Claude,
                kind,
                "default".into(),
                credential,
            )
            .unwrap();
        (root, store, account.id)
    }

    /// Each call the fake ssh saw: its remote command and the stdin it was given.
    type Calls = Arc<Mutex<Vec<(String, Option<String>)>>>;
    fn fake(
        calls: &Calls,
        add: bool,
        usage: bool,
    ) -> impl Fn(&[String], Option<&[u8]>) -> Result<String, String> + '_ {
        move |args: &[String], stdin: Option<&[u8]>| {
            assert_eq!(args[0], "studio.local");
            assert_eq!(args[1], "--");
            let command = args[2].clone();
            calls.lock().unwrap().push((
                command.clone(),
                stdin.map(|b| String::from_utf8_lossy(b).into_owned()),
            ));
            if command.contains("'accounts' 'add'") {
                return if add {
                    Ok(r#"{"ok":true,"data":{"id":"remote-1"}}"#.into())
                } else {
                    Err("ssh failed.".into())
                };
            }
            if command.contains("'usage'") {
                return if usage {
                    Ok(r#"{"ok":true,"data":{}}"#.into())
                } else {
                    Err("ssh failed.".into())
                };
            }
            Ok(r#"{"ok":true,"data":null}"#.into())
        }
    }

    #[test]
    fn a_sign_in_moves_once_with_its_tokens_on_stdin_and_this_machine_stops_using_it() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        let answer = send(
            &store,
            &id,
            "studio.local",
            "agents",
            REMOTE_DEFAULT,
            100,
            &fake(&calls, true, true),
        )
        .unwrap();
        assert_eq!(answer["remote_id"], "remote-1");
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        let (add, stdin) = &calls[0];
        assert!(
            add.starts_with("'/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard'")
        );
        assert!(add.contains("'--label' 'Work O'\\''Neil' '--pool' 'agents' '--secret-stdin'"));
        assert!(!add.contains("synthetic"), "no token in the command line");
        assert!(stdin.as_deref().unwrap().contains("synthetic-refresh"));
        assert!(calls[1].0.contains("'usage' 'remote-1'"));
        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.elsewhere.contains_key(&id));
        assert_eq!(
            snapshot.locations.get(&id).map(String::as_str),
            Some("studio.local")
        );
    }

    #[test]
    fn a_failed_save_there_leaves_the_account_here() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        assert_eq!(
            send(
                &store,
                &id,
                "studio.local",
                "default",
                REMOTE_DEFAULT,
                100,
                &fake(&calls, false, true)
            )
            .unwrap_err(),
            ADD_FAILED
        );
        let snapshot = store.snapshot().unwrap();
        assert!(!snapshot.elsewhere.contains_key(&id));
        assert!(snapshot.accounts.iter().any(|a| a.id == id && a.enabled));
    }

    #[test]
    fn a_sign_in_the_provider_does_not_answer_there_is_removed_there_and_stays_here() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        assert_eq!(
            send(
                &store,
                &id,
                "studio.local",
                "default",
                REMOTE_DEFAULT,
                100,
                &fake(&calls, true, false)
            )
            .unwrap_err(),
            NOT_VERIFIED
        );
        assert!(calls.lock().unwrap()[2]
            .0
            .contains("'accounts' 'remove' 'remote-1'"));
        let snapshot = store.snapshot().unwrap();
        assert!(!snapshot.elsewhere.contains_key(&id));
        assert!(snapshot.accounts.iter().any(|a| a.id == id && a.enabled));
    }

    #[test]
    fn only_a_subscription_sign_in_to_a_named_machine_moves() {
        let calls: Calls = Default::default();
        let (_root, store, key) = store_with(AuthKind::ApiKey);
        assert_eq!(
            send(
                &store,
                &key,
                "studio.local",
                "default",
                REMOTE_DEFAULT,
                100,
                &fake(&calls, true, true)
            )
            .unwrap_err(),
            NOT_A_SIGN_IN
        );
        let (_root, store, id) = store_with(AuthKind::OAuth);
        for target in ["-oProxyCommand=x", "a b", "", "host;rm"] {
            assert_eq!(
                send(
                    &store,
                    &id,
                    target,
                    "default",
                    REMOTE_DEFAULT,
                    100,
                    &fake(&calls, true, true)
                )
                .unwrap_err(),
                TARGET_INVALID
            );
        }
        assert_eq!(
            send(
                &store,
                &id,
                "studio.local",
                "default",
                "switchboard",
                100,
                &fake(&calls, true, true)
            )
            .unwrap_err(),
            REMOTE_INVALID
        );
        assert!(calls.lock().unwrap().is_empty(), "nothing ran");
    }
}
