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
/// The other machine's own answer to `accounts add` when the same sign-in already sits in that
/// group there (`switchboard_core`, "Account credential already exists in this pool"): a copy
/// left by an earlier move. Named, because the person can fix it there.
pub const ALREADY_THERE: &str = "The other machine already holds this sign-in in that group. Remove its old copy there, then move it again.";
pub const NOT_VERIFIED: &str = "The other machine saved the sign-in but the provider did not answer it there. It was removed there and stays here.";
pub const IN_USE_HERE: &str = "The ordinary Claude Code or Codex here is signed in to this account. Sign it in to another account first.";
pub const NOT_ONLY_COPY: &str = "This sign-in is also saved in another group here. Remove or move that copy first, so only one machine renews it.";
pub const UNREACHABLE: &str = "The other machine's Switchboard did not answer over ssh. Nothing changed; check ssh and that Switchboard runs there.";
pub const POOL_MISSING: &str =
    "The other machine has no group with that id. Name one of its groups, or default.";
pub const RENEWED_DURING: &str = "This machine renewed the sign-in while it moved, so the copy sent was no longer valid. It was removed there and stays here; move it again.";
pub const OUTCOME_UNKNOWN: &str = "The other machine's answer was lost. This machine no longer uses the account; look at the other machine's accounts and call it back or remove its copy there.";
pub const LEFT_THERE: &str = "The copy on the other machine could not be removed. This machine no longer uses the account; remove that copy there, then mark the account used here.";
/// The location a surrender writes; `--undo` takes back only this mark.
pub const CALLED_BACK: &str = "called back";
pub const NOT_CALLED_BACK: &str =
    "This account was not given back by a call back here, so its mark stays. Nothing changed.";
pub const ONLY_ONE_HERE: &str = "This is the only account its group uses on this machine. Add another account to that group before calling this one back.";
pub const USED_HERE_SINCE: &str = "This account was renewed here after it moved, so its copy there is no longer valid. Remove that copy there instead of calling it back.";

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
/// The other machine's refusal (`{"ok":false,"error":...}`), from its output whether ssh ended
/// with success or not.
fn refusal(output: &str) -> Option<String> {
    let value: Value = serde_json::from_str(output.lines().last()?.trim()).ok()?;
    (value["ok"] == false)
        .then(|| value["error"].as_str().map(str::to_owned))
        .flatten()
}

/// SB-312: the other machine's ids for its accounts, or None when it did not answer.
fn remote_ids(output: &str) -> Option<Vec<String>> {
    let accounts = data(output)?["accounts"].as_array()?.clone();
    Some(
        accounts
            .iter()
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .collect(),
    )
}
/// SB-312: a short fingerprint of a refresh token, kept with a move (`seats.jsonl`) so a call back
/// can tell whether this machine renewed the sign-in after it left. Never the token.
pub fn token_print(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(token.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Moves one sign-in. `settle` is called with the account's refresh token once it is marked used
/// elsewhere here: it returns when no renewal of that token is on the network (SB-312), so the
/// tokens read after it are the ones this machine will never renew again.
#[allow(clippy::too_many_arguments)]
pub fn send(
    store: &Store,
    id: &str,
    target: &str,
    pool: &str,
    program: &str,
    now: i64,
    run: &Run<'_>,
    settle: &dyn Fn(&str),
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
    // SB-312: the ordinary Claude Code or Codex renews its own sign-in, and another saved copy
    // of the same sign-in takes every renewal here: either would spend the tokens that left.
    if store.is_in_use(id) {
        return Err(IN_USE_HERE.into());
    }
    let credential = store.stored_credential(id)?;
    // Two known identities decide alone; a row without one is compared by its refresh token, a
    // Keychain read each, only for those rows.
    let same = |a: &switchboard_core::Account| {
        let known = |x: &switchboard_core::Account| {
            x.external_identity
                .as_ref()
                .filter(|i| i.account_id.is_some())
                .cloned()
        };
        match (known(a), known(account)) {
            (Some(x), Some(y)) => {
                x.account_id == y.account_id && x.organization_id == y.organization_id
            }
            _ => store
                .stored_credential(&a.id)
                .ok()
                .and_then(|c| c.refresh_token)
                .is_some_and(|t| credential.refresh_token.as_deref() == Some(t.as_str())),
        }
    };
    if snapshot.accounts.iter().any(|a| {
        a.id != id
            && a.provider == account.provider
            && a.kind == AuthKind::OAuth
            && a.enabled
            && !snapshot.elsewhere.contains_key(&a.id)
            && same(a)
    }) {
        return Err(NOT_ONLY_COPY.into());
    }
    let provider = match account.provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    };
    let ssh = |command: String| vec![target.to_owned(), "--".to_owned(), command];
    // SB-312: before anything is marked, the other machine answers, has the group, and its
    // accounts are read, so a lost answer to the save can be settled by looking.
    let groups = run(&ssh(remote(program, &["--json", "project", "list"])), None)
        .ok()
        .and_then(|out| data(&out))
        .ok_or(UNREACHABLE)?;
    let known = pool == "default"
        || groups["projects"]
            .as_array()
            .is_some_and(|p| p.iter().any(|g| g["pool"] == pool));
    if !known {
        return Err(POOL_MISSING.into());
    }
    let before = run(&ssh(remote(program, &["--json", "accounts", "list"])), None)
        .ok()
        .and_then(|out| remote_ids(&out))
        .ok_or(UNREACHABLE)?;
    // From here this machine never renews it: the tokens leave with one holder.
    store.mark_elsewhere_at(id, true, now, Some(target))?;
    let undo_here = || store.mark_elsewhere(id, false, now);
    if let Some(token) = credential.refresh_token.as_deref() {
        settle(token);
    }
    // Read again: a renewal that was on the network when the mark was set has stored its successor.
    let credential = match store.stored_credential(id) {
        Ok(c) => c,
        Err(e) => {
            undo_here()?;
            return Err(e);
        }
    };
    let sent = credential.refresh_token.clone();
    let remove_there = |remote_id: &str| {
        run(
            &ssh(remote(
                program,
                &["--json", "accounts", "remove", remote_id],
            )),
            None,
        )
        .ok()
        .and_then(|out| data(&out))
        .is_some()
    };
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
    );
    let output = match &added {
        Ok(out) | Err(out) => out.as_str(),
    };
    let remote_id = match data(output).and_then(|d| d["id"].as_str().map(str::to_owned)) {
        Some(remote_id) => remote_id,
        None => {
            if let Some(error) = refusal(output) {
                undo_here()?;
                // The person's sentence is in `error`, the backend's own text in `raw`.
                let duplicate = output.contains("already exists in this pool")
                    || error.contains("already exists in this pool");
                return Err(if duplicate { ALREADY_THERE } else { ADD_FAILED }.into());
            }
            // No answer: look whether the other machine saved it before deciding who holds it.
            let after = run(&ssh(remote(program, &["--json", "accounts", "list"])), None)
                .ok()
                .and_then(|out| remote_ids(&out));
            return match after {
                Some(after) => {
                    let new: Vec<&String> = after.iter().filter(|a| !before.contains(a)).collect();
                    if new.is_empty() {
                        undo_here()?;
                        Err(ADD_FAILED.into())
                    } else if new.len() == 1 && remove_there(new[0]) {
                        undo_here()?;
                        Err(ADD_FAILED.into())
                    } else {
                        Err(OUTCOME_UNKNOWN.into())
                    }
                }
                None => Err(OUTCOME_UNKNOWN.into()),
            };
        }
    };
    let verified = run(
        &ssh(remote(program, &["--json", "usage", &remote_id])),
        None,
    )
    .ok()
    .and_then(|out| data(&out))
    .is_some();
    // SB-312: the sign-in renewed here while it moved is no longer valid there.
    let renewed_here = store
        .stored_credential(id)
        .ok()
        .is_some_and(|c| c.refresh_token != sent);
    if !verified || renewed_here {
        if !remove_there(&remote_id) {
            return Err(LEFT_THERE.into());
        }
        undo_here()?;
        return Err(if renewed_here {
            RENEWED_DURING
        } else {
            NOT_VERIFIED
        }
        .into());
    }
    Ok(json!({
        "moved": true, "target": target, "remote_id": remote_id, "marked_elsewhere": true,
        "sent": sent.as_deref().map(token_print),
    }))
}

/// SB-292 (S-3), on the machine that holds the account: give its sign-in back to the machine that
/// calls it back. It is marked here as used elsewhere first, so this machine never renews it
/// again, and its tokens are answered for the caller's ssh pipe (the CLI refuses a terminal).
/// `undo` clears the mark when the caller could not take it.
pub fn surrender(
    store: &Store,
    id: &str,
    undo: bool,
    now: i64,
    settle: &dyn Fn(&str),
) -> Result<Value, String> {
    if undo {
        // SB-312: only the mark a surrender set is taken back, never one a move or a person set.
        let snapshot = store.snapshot()?;
        if !snapshot.elsewhere.contains_key(id) {
            return Ok(json!({"restored": false}));
        }
        if snapshot.locations.get(id).map(String::as_str) != Some(CALLED_BACK) {
            return Err(NOT_CALLED_BACK.into());
        }
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
    if store.is_in_use(id) {
        return Err(IN_USE_HERE.into());
    }
    // SB-312: a group's selected account is the normal state after a move (switching is on by
    // default), so its group moves to another account of its own here instead of refusing.
    for (key, value) in &snapshot.routes {
        if value != id {
            continue;
        }
        let pool = key.split_once(':').map_or(key.as_str(), |(_, p)| p);
        let next = snapshot.accounts.iter().find(|a| {
            a.id != id
                && a.provider == account.provider
                && a.pool == pool
                && a.enabled
                && !snapshot.elsewhere.contains_key(&a.id)
        });
        match next {
            Some(next) => store.select(account.provider, pool, &next.id)?,
            None => return Err(ONLY_ONE_HERE.into()),
        }
    }
    store.mark_elsewhere_at(id, true, now, Some(CALLED_BACK))?;
    let credential = match store.stored_credential(id) {
        Ok(c) => c,
        Err(e) => {
            store.mark_elsewhere(id, false, now)?;
            return Err(e);
        }
    };
    if let Some(token) = credential.refresh_token.as_deref() {
        settle(token);
    }
    let credential = store.stored_credential(id)?;
    let tokens: Value =
        serde_json::from_str(&credential.to_oauth_json(account.provider)).unwrap_or(Value::Null);
    Ok(json!({"provider": "claude", "credential": tokens}))
}

/// What `recall_take` brought back, for `recall_undo` when the provider does not answer it here.
pub struct Taken {
    pub remote_id: String,
    /// The refresh token taken back, to tell a renewal here from no answer.
    pub token: Option<String>,
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
    sent: Option<&str>,
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
    // SB-312: renewed here after it left (*Use here*, then a renewal), the copy there is spent and
    // would overwrite the live one here.
    if sent.is_some_and(|print| token_print(&spent) != print) {
        return Err(USED_HERE_SINCE.into());
    }
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
    let answer = run(
        &ssh(remote(
            program,
            &["--json", "accounts", "surrender", remote_id],
        )),
        None,
    );
    let output = match &answer {
        Ok(out) | Err(out) => out.as_str(),
    };
    let Some(given) = data(output) else {
        // SB-312: the other machine's own reason, when it gave one; a lost answer may have
        // marked it there, so that mark is taken back.
        if let Some(reason) = refusal(output) {
            return Err(reason);
        }
        undo_there();
        return Err(SURRENDER_FAILED.into());
    };
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
        token: credential.refresh_token.clone(),
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

/// The real runner: the system's `ssh` in batch mode (no prompt), 2 minutes at most. Standard
/// output and standard error are read while it runs (SB-312: an answer larger than the pipe
/// stalled it until the deadline), at most 1 MiB and 64 KiB; on a failure both are returned,
/// since the other switchboard prints its refusal (`{"ok":false,...}`) on standard error.
pub fn ssh_runner(args: &[String], stdin: Option<&[u8]>) -> Result<String, String> {
    use std::io::{Read, Write};
    let program = crate::launch::find_program("ssh").ok_or("ssh is not installed here.")?;
    let mut child = std::process::Command::new(program)
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=15"])
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "ssh could not start.".to_string())?;
    let reader = |pipe: Option<Box<dyn Read + Send>>, cap: u64| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(pipe) = pipe {
                let mut limited = pipe.take(cap);
                let _ = limited.read_to_end(&mut out);
                // Drain the rest so the other end never blocks on a full pipe.
                let _ = std::io::copy(&mut limited.into_inner(), &mut std::io::sink());
            }
            String::from_utf8_lossy(&out).into_owned()
        })
    };
    let stdout = reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        1 << 20,
    );
    let stderr = reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        64 << 10,
    );
    if let Some(mut input) = child.stdin.take() {
        if let Some(bytes) = stdin {
            let _ = input.write_all(bytes);
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|_| "ssh failed.".to_string())? {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("ssh timed out.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    if status.success() {
        return Ok(out);
    }
    // The other switchboard ends with status 1 on a refusal and prints why: keep that.
    let both = format!("{out}\n{err}");
    let last_json = both
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .unwrap_or("")
        .to_owned();
    if refusal(&last_json).is_some() {
        Err(last_json)
    } else {
        Err("ssh failed.".into())
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
            if command.contains("'project' 'list'") {
                return Ok(r#"{"ok":true,"data":{"projects":[{"pool":"agents"}]}}"#.into());
            }
            if command.contains("'accounts' 'list'") {
                return Ok(r#"{"ok":true,"data":{"accounts":[]}}"#.into());
            }
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
            &|_| {},
        )
        .unwrap();
        assert_eq!(answer["remote_id"], "remote-1");
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert!(calls[0].0.contains("'project' 'list'"));
        assert!(calls[1].0.contains("'accounts' 'list'"));
        let (add, stdin) = &calls[2];
        assert!(
            add.starts_with("'/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard'")
        );
        assert!(add.contains("'--label' 'Work O'\\''Neil' '--pool' 'agents' '--secret-stdin'"));
        assert!(!add.contains("synthetic"), "no token in the command line");
        assert!(stdin.as_deref().unwrap().contains("synthetic-refresh"));
        assert!(calls[3].0.contains("'usage' 'remote-1'"));
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
                &fake(&calls, false, true),
                &|_| {}
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
                &fake(&calls, true, false),
                &|_| {}
            )
            .unwrap_err(),
            NOT_VERIFIED
        );
        assert!(calls.lock().unwrap()[4]
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
                &fake(&calls, true, true),
                &|_| {}
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
                    &fake(&calls, true, true),
                    &|_| {}
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
                &fake(&calls, true, true),
                &|_| {}
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
    fn a_copy_left_there_by_an_earlier_move_is_named_and_nothing_changes_here() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let preflight = |command: &str| -> Option<Result<String, String>> {
            if command.contains("'project' 'list'") {
                return Some(Ok(
                    r#"{"ok":true,"data":{"projects":[{"pool":"agents"}]}}"#.into()
                ));
            }
            if command.contains("'accounts' 'list'") {
                return Some(Ok(r#"{"ok":true,"data":{"accounts":[]}}"#.into()));
            }
            None
        };
        // The person's sentence in `error`, the backend's text in `raw` (SB-312).
        let refused = |args: &[String], _stdin: Option<&[u8]>| -> Result<String, String> {
            if let Some(answer) = preflight(&args[2]) {
                return answer;
            }
            assert!(
                args[2].contains("'accounts' 'add'"),
                "nothing runs after the refusal"
            );
            Err(r#"{"error":"This sign-in is already saved in that group.","ok":false,"raw":"Account credential already exists in this pool"}"#.into())
        };
        assert_eq!(
            send(
                &store,
                &id,
                "studio.local",
                "agents",
                REMOTE_DEFAULT,
                5,
                &refused,
                &|_| {}
            )
            .unwrap_err(),
            ALREADY_THERE
        );
        assert!(
            !store.snapshot().unwrap().elsewhere.contains_key(&id),
            "undone here"
        );
        let other = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            if let Some(answer) = preflight(&args[2]) {
                return answer;
            }
            Err(r#"{"error":"Credential storage unavailable","ok":false}"#.into())
        };
        assert_eq!(
            send(
                &store,
                &id,
                "studio.local",
                "agents",
                REMOTE_DEFAULT,
                6,
                &other,
                &|_| {}
            )
            .unwrap_err(),
            ADD_FAILED
        );
        assert_eq!(
            refusal("noise\n{\"ok\":false,\"error\":\"x\"}"),
            Some("x".into())
        );
        assert_eq!(refusal("{\"ok\":true,\"data\":{}}"), None);
    }

    /// SB-312: the answers of a reachable machine with the group `agents`, then `rest`.
    fn studio<'a>(
        calls: &'a Calls,
        after_add: Option<&'static str>,
        rest: impl Fn(&str) -> Result<String, String> + 'a,
    ) -> impl Fn(&[String], Option<&[u8]>) -> Result<String, String> + 'a {
        let lists = std::sync::atomic::AtomicUsize::new(0);
        move |args: &[String], _stdin: Option<&[u8]>| {
            let command = args[2].clone();
            calls.lock().unwrap().push((command.clone(), None));
            if command.contains("'project' 'list'") {
                return Ok(r#"{"ok":true,"data":{"projects":[{"pool":"agents"}]}}"#.into());
            }
            if command.contains("'accounts' 'list'") {
                let second = lists.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0;
                return match (second, after_add) {
                    (true, Some(list)) => Ok(list.into()),
                    (true, None) => Err("ssh timed out.".into()),
                    (false, _) => Ok(r#"{"ok":true,"data":{"accounts":[{"id":"old"}]}}"#.into()),
                };
            }
            rest(&command)
        }
    }

    #[test]
    fn a_move_needs_the_other_machine_and_its_group_before_anything_is_marked() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        let silent = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err("ssh failed.".into())
        };
        let send_to = |pool: &str, run: &Run<'_>| {
            send(
                &store,
                &id,
                "studio.local",
                pool,
                REMOTE_DEFAULT,
                1,
                run,
                &|_| {},
            )
        };
        assert_eq!(send_to("agents", &silent).unwrap_err(), UNREACHABLE);
        assert_eq!(
            send_to(
                "nowhere",
                &studio(&calls, None, |_| Err("unexpected".into()))
            )
            .unwrap_err(),
            POOL_MISSING
        );
        assert!(
            store.snapshot().unwrap().elsewhere.is_empty(),
            "nothing marked"
        );
    }

    #[test]
    fn a_lost_answer_to_the_save_is_settled_by_looking_there() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        let lost = |command: &str| -> Result<String, String> {
            if command.contains("'accounts' 'remove' 'new-1'") {
                return Ok(r#"{"ok":true,"data":null}"#.into());
            }
            assert!(command.contains("'accounts' 'add'"), "{command}");
            Err("ssh timed out.".into())
        };
        // It was saved there after all: that copy is removed and the account stays here.
        let saved = studio(
            &calls,
            Some(r#"{"ok":true,"data":{"accounts":[{"id":"old"},{"id":"new-1"}]}}"#),
            lost,
        );
        let err = send(
            &store,
            &id,
            "studio.local",
            "agents",
            REMOTE_DEFAULT,
            1,
            &saved,
            &|_| {},
        );
        assert_eq!(err.unwrap_err(), ADD_FAILED);
        assert!(calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c.contains("'remove' 'new-1'")));
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        // Nobody can tell: this machine keeps not using it, so at most one machine holds it.
        let calls: Calls = Default::default();
        let unknown = studio(&calls, None, lost);
        let err = send(
            &store,
            &id,
            "studio.local",
            "agents",
            REMOTE_DEFAULT,
            2,
            &unknown,
            &|_| {},
        );
        assert_eq!(err.unwrap_err(), OUTCOME_UNKNOWN);
        assert!(store.snapshot().unwrap().elsewhere.contains_key(&id));
    }

    #[test]
    fn a_sign_in_renewed_here_while_it_moved_is_taken_back_there() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        let settled = std::sync::atomic::AtomicBool::new(false);
        let renewing = studio(&calls, None, |command| {
            if command.contains("'accounts' 'add'") {
                // A renewal reserved before the mark stores its successor now.
                let successor = Credential {
                    access_token: "synthetic-access-2".into(),
                    refresh_token: Some("synthetic-refresh-2".into()),
                    id_token: None,
                    native_context: None,
                    expires_at: Some(2_000_000_100),
                    account_id: Some("synthetic-account".into()),
                };
                store
                    .adopt_refreshed(Provider::Claude, "synthetic-refresh", &successor)
                    .unwrap();
                return Ok(r#"{"ok":true,"data":{"id":"remote-1"}}"#.into());
            }
            Ok(r#"{"ok":true,"data":{}}"#.into())
        });
        let err = send(
            &store,
            &id,
            "studio.local",
            "agents",
            REMOTE_DEFAULT,
            1,
            &renewing,
            &|token| {
                assert_eq!(
                    token, "synthetic-refresh",
                    "settled on the token being sent"
                );
                settled.store(true, std::sync::atomic::Ordering::SeqCst);
            },
        );
        assert_eq!(err.unwrap_err(), RENEWED_DURING);
        assert!(settled.load(std::sync::atomic::Ordering::SeqCst));
        assert!(calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c.contains("'remove' 'remote-1'")));
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
    }

    #[test]
    fn another_copy_of_the_sign_in_or_the_cli_login_here_keeps_it_here() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        let ok = studio(&calls, None, |_| Err("unexpected".into()));
        store
            .set_in_use(std::collections::BTreeSet::from([id.clone()]))
            .unwrap();
        let err = send(
            &store,
            &id,
            "studio.local",
            "agents",
            REMOTE_DEFAULT,
            1,
            &ok,
            &|_| {},
        );
        assert_eq!(err.unwrap_err(), IN_USE_HERE);
        store.set_in_use(Default::default()).unwrap();
        // The same sign-in saved in another group here.
        store
            .add(
                "Copy".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "agents".into(),
                store.stored_credential(&id).unwrap(),
            )
            .unwrap();
        let err = send(
            &store,
            &id,
            "studio.local",
            "agents",
            REMOTE_DEFAULT,
            2,
            &ok,
            &|_| {},
        );
        assert_eq!(err.unwrap_err(), NOT_ONLY_COPY);
        assert!(calls.lock().unwrap().is_empty(), "nothing ran");
    }

    #[test]
    fn a_selected_account_hands_its_group_to_another_before_it_is_given_back() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store.select(Provider::Claude, "default", &id).unwrap();
        assert_eq!(
            surrender(&store, &id, false, 7, &|_| {}).unwrap_err(),
            ONLY_ONE_HERE
        );
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        let other = store
            .add(
                "Other".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                Credential {
                    access_token: "other-access".into(),
                    refresh_token: Some("other-refresh".into()),
                    id_token: None,
                    native_context: None,
                    expires_at: Some(2_000_000_000),
                    account_id: None,
                },
            )
            .unwrap();
        surrender(&store, &id, false, 8, &|_| {}).unwrap();
        let snapshot = store.snapshot().unwrap();
        assert_eq!(snapshot.routes.get("claude:default"), Some(&other.id));
        assert!(snapshot.elsewhere.contains_key(&id));
        // Its undo takes back only the mark a surrender set.
        store
            .mark_elsewhere_at(&other.id, true, 9, Some("studio"))
            .unwrap();
        assert_eq!(
            surrender(&store, &other.id, true, 10, &|_| {}).unwrap_err(),
            NOT_CALLED_BACK
        );
        surrender(&store, &id, true, 11, &|_| {}).unwrap();
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
    }

    #[test]
    fn a_sign_in_renewed_here_after_it_moved_is_not_called_back() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store
            .mark_elsewhere_at(&id, true, 1, Some("studio.local"))
            .unwrap();
        let calls: Calls = Default::default();
        let print = token_print("an-older-token");
        assert_eq!(
            recall_take(
                &store,
                &id,
                "studio.local",
                "remote-1",
                REMOTE_DEFAULT,
                2,
                &member(&calls, None, false),
                Some(&print),
            )
            .err(),
            Some(USED_HERE_SINCE.to_string())
        );
        assert!(calls.lock().unwrap().is_empty(), "nothing asked there");
        assert_eq!(token_print("synthetic-refresh").len(), 16);
    }

    #[test]
    fn a_member_gives_a_sign_in_back_once_and_never_to_a_terminal_of_its_own() {
        // SB-292 (S-3): the giving side marks it used elsewhere before the tokens leave.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let given = surrender(&store, &id, false, 7, &|_| {}).unwrap();
        assert_eq!(given["provider"], "claude");
        let back = Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &given["credential"].to_string(),
        )
        .unwrap();
        assert_eq!(back.refresh_token.as_deref(), Some("synthetic-refresh"));
        assert!(store.snapshot().unwrap().elsewhere.contains_key(&id));
        assert_eq!(
            surrender(&store, &id, false, 8, &|_| {}).unwrap_err(),
            NOT_HERE
        );
        surrender(&store, &id, true, 9, &|_| {}).unwrap();
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        let (_root, keyed, key) = store_with(AuthKind::ApiKey);
        assert_eq!(
            surrender(&keyed, &key, false, 7, &|_| {}).unwrap_err(),
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
                &member(&calls, Some(RENEWED), false),
                None
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
            None,
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
                &member(&calls, None, false),
                None
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
            &member(&calls, Some(r#"{"x":1}"#), false),
            None
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
            None,
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
                &member(&calls, Some(RENEWED), false),
                None
            )
            .err()
            .unwrap(),
            NO_REMOTE_ID
        );
    }
}
