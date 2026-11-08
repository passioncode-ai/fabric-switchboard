//! SB-283: move an account's sign-in to another machine over ssh, without a new sign-in there.
//!
//! A sign-in has one holder: a renewal on one machine ends the other's
//! (docs/research/two-machines-2026-10-23.md, D-2). So this machine marks the account as used
//! elsewhere first (it never renews it again, SB-293), then hands the provider's tokens to the
//! other machine's `switchboard accounts add` on stdin, checks there that the provider answers,
//! and undoes both on any failure. Raised by the operator 2026-11-07 («поднимай», relayed by
//! studio-agent, board thread `f32a24c0`).

use serde_json::{json, Value};
use switchboard_core::{AuthKind, Credential, Provider, Store};

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

pub const CLAUDE_ONLY: &str = "Only a Claude sign-in can be called back for now.";
pub const NOT_AWAY: &str = "This account is used here, not on another machine.";
pub const NO_REMOTE_ID: &str =
    "Name the account's id on the other machine; no recorded move names it.";
pub const SURRENDER_FAILED: &str =
    "The other machine did not give the sign-in back. Nothing changed; check ssh and its Switchboard.";
pub const NOT_RETURNED: &str = "The sign-in came back but the provider did not answer it here. It went back to the other machine.";
pub const SURRENDER_TO_TERMINAL: &str =
    "This prints a sign-in. Run it only through ssh from the machine that calls the account back.";

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

/// SB-292 (S-3), on the machine that holds the account: give its sign-in back to the machine that
/// calls it back. It is marked here as used elsewhere first, so this machine never renews it
/// again, and its tokens are answered for the caller's ssh pipe (the CLI refuses a terminal).
/// `undo` clears the mark when the caller could not take it.
pub fn surrender(store: &Store, id: &str, undo: bool, now: i64) -> Result<Value, String> {
    if undo {
        store.mark_elsewhere(id, false, now)?;
        return Ok(json!({"restored": true}));
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
    if account.provider != Provider::Claude {
        return Err(CLAUDE_ONLY.into());
    }
    if !account.enabled || snapshot.elsewhere.contains_key(id) {
        return Err(NOT_HERE.into());
    }
    if snapshot.routes.values().any(|v| v == id) {
        return Err(SELECTED.into());
    }
    let credential = store.stored_credential(id)?;
    store.mark_elsewhere_at(id, true, now, Some("called back"))?;
    let tokens: Value =
        serde_json::from_str(&credential.to_oauth_json(account.provider)).unwrap_or(Value::Null);
    Ok(json!({"provider": "claude", "credential": tokens}))
}

/// What `recall_take` brought back, for `recall_undo` when the provider does not answer it here.
pub struct Taken {
    pub remote_id: String,
}

/// SB-292 (S-3), on this machine: take the sign-in of an account this machine moved away
/// (`send`) back from `target`. The other machine marks it used elsewhere and hands its tokens
/// over ssh; they replace the spent ones here (`Store::adopt_refreshed`, identity checked) and
/// the mark here is cleared. The caller checks the provider answers, then `recall_finish`.
pub fn recall_take(
    store: &Store,
    id: &str,
    target: &str,
    remote_id: &str,
    program: &str,
    now: i64,
    run: &Run<'_>,
) -> Result<Taken, String> {
    if !target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    if !program.starts_with('/') || program.contains(['\'', '"', '\n']) {
        return Err(REMOTE_INVALID.into());
    }
    if remote_id.is_empty() || remote_id.len() > 64 {
        return Err(NO_REMOTE_ID.into());
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
    if account.provider != Provider::Claude {
        return Err(CLAUDE_ONLY.into());
    }
    if !snapshot.elsewhere.contains_key(id) {
        return Err(NOT_AWAY.into());
    }
    let spent = store
        .stored_credential(id)?
        .refresh_token
        .ok_or(NOT_A_SIGN_IN)?;
    let ssh = |command: String| vec![target.to_owned(), "--".to_owned(), command];
    let undo_there = || {
        let _ = run(
            &ssh(remote(
                program,
                &["--json", "accounts", "surrender", remote_id, "--undo"],
            )),
            None,
        );
    };
    let given = run(
        &ssh(remote(
            program,
            &["--json", "accounts", "surrender", remote_id],
        )),
        None,
    )
    .ok()
    .and_then(|out| data(&out))
    .ok_or(SURRENDER_FAILED)?;
    let tokens = given["credential"].to_string();
    let Ok(credential) = Credential::parse(Provider::Claude, AuthKind::OAuth, &tokens) else {
        undo_there();
        return Err(SURRENDER_FAILED.into());
    };
    if store
        .adopt_refreshed(Provider::Claude, &spent, &credential)
        .is_err()
    {
        undo_there();
        return Err(SURRENDER_FAILED.into());
    }
    store.mark_elsewhere(id, false, now)?;
    Ok(Taken {
        remote_id: remote_id.to_owned(),
    })
}

/// The provider did not answer the sign-in here: it goes back to the other machine, which holds
/// the same tokens and renews them again, and this machine stops using it.
pub fn recall_undo(
    store: &Store,
    id: &str,
    target: &str,
    taken: &Taken,
    program: &str,
    now: i64,
    run: &Run<'_>,
) -> Result<(), String> {
    store.mark_elsewhere_at(id, true, now, Some(target))?;
    let command = remote(
        program,
        &[
            "--json",
            "accounts",
            "surrender",
            &taken.remote_id,
            "--undo",
        ],
    );
    let _ = run(&[target.to_owned(), "--".to_owned(), command], None);
    Ok(())
}

/// SB-292 (S-4): the sign-in works here; remove the other machine's copy and prove it is gone:
/// its account list no longer names it. Answers `{recalled, removed_there, proved}`.
pub fn recall_finish(target: &str, taken: &Taken, program: &str, run: &Run<'_>) -> Value {
    let ssh = |command: String| vec![target.to_owned(), "--".to_owned(), command];
    let removed = run(
        &ssh(remote(
            program,
            &["--json", "accounts", "remove", &taken.remote_id],
        )),
        None,
    )
    .ok()
    .and_then(|out| data(&out))
    .is_some();
    let proved = removed
        && run(&ssh(remote(program, &["--json", "accounts", "list"])), None)
            .ok()
            .and_then(|out| data(&out))
            .and_then(|d| d["accounts"].as_array().cloned())
            .is_some_and(|accounts| accounts.iter().all(|a| a["id"] != taken.remote_id.as_str()));
    json!({"recalled": true, "target": target, "removed_there": removed, "proved": proved})
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
            if let Some(stdout) = child.stdout.take() {
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

    /// The other machine's answers for a call back: `surrender` gives these tokens (or fails),
    /// `remove` works, and `list` names `still` (the copy was not removed) or nothing.
    fn member<'a>(
        calls: &'a Calls,
        give: Option<&'static str>,
        still: bool,
    ) -> impl Fn(&[String], Option<&[u8]>) -> Result<String, String> + 'a {
        move |args: &[String], _stdin: Option<&[u8]>| {
            assert_eq!(args[0], "studio.local");
            let command = args[2].clone();
            calls.lock().unwrap().push((command.clone(), None));
            if command.contains("'surrender'") && command.contains("'--undo'") {
                return Ok(r#"{"ok":true,"data":{"restored":true}}"#.into());
            }
            if command.contains("'surrender'") {
                return give
                    .map(|tokens| {
                        format!(
                            r#"{{"ok":true,"data":{{"provider":"claude","credential":{tokens}}}}}"#
                        )
                    })
                    .ok_or_else(|| "ssh failed.".to_string());
            }
            if command.contains("'remove'") {
                return Ok(r#"{"ok":true,"data":null}"#.into());
            }
            if command.contains("'list'") {
                let accounts = if still {
                    r#"[{"id":"remote-1"}]"#
                } else {
                    "[]"
                };
                return Ok(format!(r#"{{"ok":true,"data":{{"accounts":{accounts}}}}}"#));
            }
            Err("unexpected call".into())
        }
    }

    #[test]
    fn a_member_gives_a_sign_in_back_once_and_never_to_a_terminal_of_its_own() {
        // SB-292 (S-3): the giving side marks it used elsewhere before the tokens leave.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let given = surrender(&store, &id, false, 7).unwrap();
        assert_eq!(given["provider"], "claude");
        let back = Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &given["credential"].to_string(),
        )
        .unwrap();
        assert_eq!(back.refresh_token.as_deref(), Some("synthetic-refresh"));
        assert!(store.snapshot().unwrap().elsewhere.contains_key(&id));
        assert_eq!(surrender(&store, &id, false, 8).unwrap_err(), NOT_HERE);
        surrender(&store, &id, true, 9).unwrap();
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        let (_root, keyed, key) = store_with(AuthKind::ApiKey);
        assert_eq!(
            surrender(&keyed, &key, false, 7).unwrap_err(),
            NOT_A_SIGN_IN
        );
    }

    const RENEWED: &str = r#"{"claudeAiOauth":{"accessToken":"synthetic-access-2","refreshToken":"synthetic-refresh-2","expiresAt":2000000000000,"accountUuid":"synthetic-account"}}"#;

    #[test]
    fn a_recalled_sign_in_replaces_the_spent_one_and_its_copy_is_proved_gone() {
        // SB-292 (S-3, S-4): the tokens the member renewed come back, the mark here clears, the
        // member's copy is removed and its list read.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls = Calls::default();
        assert_eq!(
            recall_take(
                &store,
                &id,
                "studio.local",
                "remote-1",
                REMOTE_DEFAULT,
                5,
                &member(&calls, Some(RENEWED), false)
            )
            .err()
            .unwrap(),
            NOT_AWAY,
            "an account used here is not called back"
        );
        store
            .mark_elsewhere_at(&id, true, 5, Some("studio.local"))
            .unwrap();
        let taken = recall_take(
            &store,
            &id,
            "studio.local",
            "remote-1",
            REMOTE_DEFAULT,
            6,
            &member(&calls, Some(RENEWED), false),
        )
        .unwrap();
        let stored = store.stored_credential(&id).unwrap();
        assert_eq!(stored.refresh_token.as_deref(), Some("synthetic-refresh-2"));
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        let done = recall_finish(
            "studio.local",
            &taken,
            REMOTE_DEFAULT,
            &member(&calls, None, false),
        );
        assert_eq!(done["removed_there"], true);
        assert_eq!(done["proved"], true);
        let kept = recall_finish(
            "studio.local",
            &taken,
            REMOTE_DEFAULT,
            &member(&calls, None, true),
        );
        assert_eq!(
            kept["proved"], false,
            "a copy still listed there is not proved gone"
        );
    }

    #[test]
    fn a_failed_call_back_changes_nothing_here_and_a_refused_one_goes_back() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store
            .mark_elsewhere_at(&id, true, 5, Some("studio.local"))
            .unwrap();
        let calls = Calls::default();
        assert_eq!(
            recall_take(
                &store,
                &id,
                "studio.local",
                "remote-1",
                REMOTE_DEFAULT,
                6,
                &member(&calls, None, false)
            )
            .err()
            .unwrap(),
            SURRENDER_FAILED
        );
        assert_eq!(
            store
                .stored_credential(&id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-refresh")
        );
        assert!(store.snapshot().unwrap().elsewhere.contains_key(&id));
        // Tokens that are not a sign-in: the member is told to use it again.
        calls.lock().unwrap().clear();
        assert!(recall_take(
            &store,
            &id,
            "studio.local",
            "remote-1",
            REMOTE_DEFAULT,
            6,
            &member(&calls, Some(r#"{"x":1}"#), false)
        )
        .is_err());
        assert!(calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c.contains("'--undo'")));
        // The provider did not answer it here: it goes back there and is used elsewhere here.
        let taken = recall_take(
            &store,
            &id,
            "studio.local",
            "remote-1",
            REMOTE_DEFAULT,
            7,
            &member(&calls, Some(RENEWED), false),
        )
        .unwrap();
        calls.lock().unwrap().clear();
        recall_undo(
            &store,
            &id,
            "studio.local",
            &taken,
            REMOTE_DEFAULT,
            8,
            &member(&calls, None, false),
        )
        .unwrap();
        assert!(store.snapshot().unwrap().elsewhere.contains_key(&id));
        assert!(calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c.contains("'surrender' 'remote-1' '--undo'")));
        assert_eq!(
            recall_take(
                &store,
                &id,
                "studio.local",
                "",
                REMOTE_DEFAULT,
                9,
                &member(&calls, Some(RENEWED), false)
            )
            .err()
            .unwrap(),
            NO_REMOTE_ID
        );
    }
}
