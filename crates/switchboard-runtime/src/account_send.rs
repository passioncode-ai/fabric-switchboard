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
/// SB-315: the Switchboard to run on the other machine: its program and, when that machine's
/// Switchboard keeps its data elsewhere (`serve --data-dir`), that folder, given to every command
/// there as `--data-dir`.
#[derive(Clone, Copy, Debug)]
pub struct Program<'a> {
    pub path: &'a str,
    pub data_dir: Option<&'a str>,
}
/// The macOS app's own Switchboard with its own data folder.
pub const DEFAULT_APP: Program<'static> = Program {
    path: REMOTE_DEFAULT,
    data_dir: None,
};
impl Program<'_> {
    /// Both are absolute paths without a quote or a line end.
    pub(crate) fn check(&self) -> Result<(), String> {
        let plain = |p: &str| p.starts_with('/') && !p.contains(['\'', '"', '\n']);
        if !plain(self.path) {
            return Err(REMOTE_INVALID.into());
        }
        if !self.data_dir.is_none_or(plain) {
            return Err(REMOTE_DATA_DIR_INVALID.into());
        }
        Ok(())
    }
}
pub const TARGET_INVALID: &str =
    "Name the other machine as ssh knows it: letters, digits, dots, hyphens, @ and : only.";
pub const REMOTE_INVALID: &str =
    "Give the other machine's switchboard as an absolute path without quotes.";
/// SB-315: `--remote-data-dir` that is not an absolute path.
pub const REMOTE_DATA_DIR_INVALID: &str =
    "Give the other machine's data folder as an absolute path without quotes.";
pub const NOT_A_SIGN_IN: &str =
    "Only a Claude or Codex subscription sign-in moves this way. Give an API key through your vault.";
pub const NOT_HERE: &str = "This account is already used elsewhere or turned off here.";
/// SB-377: the group a selected account leaves keeps working only with another account here.
/// SB-377: the account the ordinary Claude Code is signed in to moves only when another can take
/// its place.
pub const CLI_ONLY_ONE: &str = "Claude Code here is signed in to this account and no other Claude account here can take its place. Add one, then move this one.";
pub const SEND_ONLY_ONE: &str = "This is the only account its group uses on this machine. Add another account to that group before moving this one.";
pub const ADD_FAILED: &str =
    "The other machine did not save the sign-in. Nothing changed here; check ssh and its Switchboard.";
/// The other machine's own answer to `accounts add` when the same sign-in already sits in that
/// group there (`switchboard_core`, "Account credential already exists in this pool"): a copy
/// left by an earlier move. Named, because the person can fix it there.
pub const ALREADY_THERE: &str = "The other machine already holds this sign-in in that group. Remove its old copy there, then move it again.";
/// SB-315: clearing the mark of an account whose sign-in a move took to another machine.
pub const MOVED_AWAY: &str = "This account's sign-in was moved to another machine. Call it back to use it here, so only one machine renews it.";
pub const NOT_VERIFIED: &str = "The other machine saved the sign-in but the provider did not answer it there. It was removed there and stays here.";
pub const IN_USE_HERE: &str = "The ordinary Claude Code or Codex here is signed in to this account. Sign it in to another account first.";
pub const NOT_ONLY_COPY: &str = "This sign-in is also saved in another group here. Remove or move that copy first, so only one machine renews it.";
pub const UNREACHABLE: &str = "The other machine's Switchboard did not answer over ssh. Nothing changed; check ssh and that Switchboard runs there.";
pub const POOL_MISSING: &str =
    "The other machine has no group with that id. Name one of its groups, or default.";
pub const RENEWED_DURING: &str = "This machine renewed the sign-in while it moved, so the copy sent was no longer valid. It was removed there and stays here; move it again.";
pub const OUTCOME_UNKNOWN: &str = "The other machine's answer was lost. This machine no longer uses the account; look at the other machine's accounts and call it back or remove its copy there.";
pub const LEFT_THERE: &str = "The copy on the other machine could not be removed. This machine no longer uses the account; remove that copy there, then mark the account used here.";
/// SB-403: the delegated copy there could not be removed after a failed check. It is switched
/// off there once its lease runs out; the guest cannot be removed by the person there.
/// The seat is recorded under its key (review round 2, P2-B), so a call back takes it back; the
/// account is not placed in the machine's group, so its lease there is not extended.
pub const LEFT_THERE_DELEGATED: &str = "The delegated copy on the other machine could not be removed. This machine no longer uses the account, and its lease there is no longer extended. Call it back with accounts recall.";
/// The location a surrender writes; `--undo` takes back only this mark.
pub const CALLED_BACK: &str = "called back";
/// SB-377: the mark of a surrender whose account was off here; `--undo` turns it off again.
pub const CALLED_BACK_OFF: &str = "called back, was off";
pub const NOT_CALLED_BACK: &str =
    "This account was not given back by a call back here, so its mark stays. Nothing changed.";
/// What a Switchboard before 0.7.94 answered when the account was the only one its group used
/// there; a newer one gives it back and leaves that group with no selected account.
pub const ONLY_ONE_HERE: &str = "This is the only account its group uses on this machine. Add another account to that group before calling this one back.";
/// SB-386: the caller's reading of that old answer: it came from the other machine.
pub const OLD_THERE: &str = "The other machine runs a Switchboard older than 0.7.94, which keeps an account its group uses alone. Update Switchboard there, then call the account back again.";

pub const NOT_AWAY: &str = "This account is used here, not on another machine.";
pub const NO_REMOTE_ID: &str =
    "Name the account's id on the other machine; no recorded move names it.";
pub const SURRENDER_FAILED: &str =
    "The other machine did not give the sign-in back. Nothing changed; check ssh and its Switchboard.";
pub const NOT_RETURNED: &str = "The sign-in came back but the provider did not answer it here. It went back to the other machine.";
pub const SURRENDER_TO_TERMINAL: &str =
    "This prints a sign-in. Only the machine that calls the account back runs it, over ssh.";
/// SB-420: the variable the calling machine's ssh command sets for one `accounts surrender`, the
/// only way that command prints a sign-in; set on that command alone, never exported.
pub const TRANSFER_ENV: &str = "SWITCHBOARD_TRANSFER";
pub const TRANSFER_SURRENDER: &str = "surrender";

/// Runs one command on the other machine: `ssh <target> -- <remote command>`, with `stdin` when
/// given; answers its standard output, or an error when it failed.
pub type Run<'a> = dyn Fn(&[String], Option<&[u8]>) -> Result<String, String> + 'a;

pub fn target_valid(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 255
        && !target.starts_with('-')
        && target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'@' | b':'))
}
/// One word for the remote shell: single-quoted, a quote inside written `'\''`.
pub use fabric_sessions_core::cli::quoted;
pub fn remote(program: &Program<'_>, args: &[&str]) -> String {
    let data_dir = program
        .data_dir
        .map(|dir| ["--data-dir", dir])
        .into_iter()
        .flatten();
    std::iter::once(program.path)
        .chain(data_dir)
        .chain(args.iter().copied())
        .map(quoted)
        .collect::<Vec<_>>()
        .join(" ")
}
/// SB-420: [`remote`] for the one call that hands a sign-in over (`accounts surrender`): it names
/// the hand-over in the command itself. An older Switchboard there ignores the variable.
fn remote_transfer(program: &Program<'_>, args: &[&str]) -> String {
    format!(
        "/usr/bin/env {TRANSFER_ENV}={TRANSFER_SURRENDER} {}",
        remote(program, args)
    )
}
pub use fabric_sessions_core::cli::data;
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

/// SB-377: the account a group moves to when `account`, its selected one, leaves this machine:
/// of the accounts serving `pool` (its own and shared, same provider and kind, on, used here, not
/// given to an agent), the one with room and the least used; with none that has room, the one
/// that will be free first (`free_at`); an account never measured comes after those with room.
pub(crate) fn next_in_group<'a>(
    snapshot: &'a switchboard_core::Snapshot,
    account: &switchboard_core::Account,
    pool: &str,
    now: i64,
) -> Option<&'a switchboard_core::Account> {
    snapshot
        .accounts
        .iter()
        .filter(|a| {
            a.id != account.id
                && a.provider == account.provider
                && a.kind == account.kind
                && a.enabled
                && snapshot.serves(a, pool)
                && !snapshot.elsewhere.contains_key(&a.id)
                && !snapshot.is_assigned(&a.id)
        })
        .min_by_key(|a| next_rank(a, now))
}
/// The order an account takes the place of one that leaves: with room now, the least used first
/// (in tenths of a percent); then never measured; then full ones, the one free first.
fn next_rank(a: &switchboard_core::Account, now: i64) -> (u8, i64, i64) {
    let used = a.usage.as_ref().and_then(|u| u.account_used_percent());
    let free = switchboard_core::free_at(a, now).unwrap_or(i64::MAX);
    match used {
        Some(u) if free <= now => (0, (u * 10.0) as i64, a.created_at),
        None => (1, 0, a.created_at),
        Some(_) => (2, free, a.created_at),
    }
}
/// SB-377: the account the ordinary Claude Code takes when the one it is signed in to moves away:
/// a Claude sign-in here that may become Claude Code's (its identity known, in no group with
/// folders of its own, not given to an agent), in the same order as a group's next account.
pub(crate) fn next_for_cli<'a>(
    snapshot: &'a switchboard_core::Snapshot,
    account: &switchboard_core::Account,
    now: i64,
) -> Option<&'a switchboard_core::Account> {
    snapshot
        .accounts
        .iter()
        .filter(|a| {
            a.id != account.id
                && a.provider == Provider::Claude
                && a.kind == AuthKind::OAuth
                && a.enabled
                && a.external_identity.is_some()
                && snapshot.project_of_pool(&a.pool).is_none()
                && !snapshot.elsewhere.contains_key(&a.id)
                && !snapshot.is_assigned(&a.id)
        })
        .min_by_key(|a| next_rank(a, now))
}

/// SB-403 (DEC-0025): a move that delegates the seat: the other machine saves it as a guest
/// pinned to `sender_key` (this machine's settings signing key) with a lease of `lease_hours`,
/// which this machine's background read of it extends.
#[derive(Clone, Copy, Debug)]
pub struct Delegate<'a> {
    pub sender_key: &'a str,
    pub lease_hours: u32,
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
    program: &Program<'_>,
    now: i64,
    run: &Run<'_>,
    settle: &dyn Fn(&str),
) -> Result<Value, String> {
    send_as(store, id, target, pool, program, now, run, settle, None)
}
/// [`send`], delegating the seat when `delegate` is given (SB-403).
#[allow(clippy::too_many_arguments)]
pub fn send_as(
    store: &Store,
    id: &str,
    target: &str,
    pool: &str,
    program: &Program<'_>,
    now: i64,
    run: &Run<'_>,
    settle: &dyn Fn(&str),
    delegate: Option<Delegate<'_>>,
) -> Result<Value, String> {
    send_with(store, id, target, pool, program, now, run, settle, delegate).map_err(|e| e.message)
}
/// A refused move: the person's sentence, and the delegated seat left on the other machine when
/// its copy failed its check and could not be removed (`LEFT_THERE_DELEGATED`).
#[derive(Debug)]
pub struct SendError {
    pub message: String,
    pub left: Option<switchboard_core::DelegatedOut>,
}
impl From<String> for SendError {
    fn from(message: String) -> Self {
        Self {
            message,
            left: None,
        }
    }
}
impl From<&str> for SendError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
/// [`send_as`], answering what a delegated seat left there was (review round 3, minor), so the
/// caller records it from what the move knew rather than from the store.
#[allow(clippy::too_many_arguments)]
pub fn send_with(
    store: &Store,
    id: &str,
    target: &str,
    pool: &str,
    program: &Program<'_>,
    now: i64,
    run: &Run<'_>,
    settle: &dyn Fn(&str),
    delegate: Option<Delegate<'_>>,
) -> Result<Value, SendError> {
    if !target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
    if let Some(d) = delegate {
        if !switchboard_core::delegation::lease_valid(d.lease_hours) {
            return Err(switchboard_core::delegation::LEASE_INVALID.into());
        }
        if pool == "default" {
            return Err(DELEGATE_INTO_GROUP.into());
        }
    }
    let snapshot = store.snapshot()?;
    let account = snapshot
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or("Account not found")?;
    // SB-403: a seat delegated here is the sender's: it is never passed on.
    if snapshot.is_delegated(id) {
        return Err(switchboard_core::delegation::DELEGATED_NOT_SENT.into());
    }
    if account.kind != AuthKind::OAuth {
        return Err(NOT_A_SIGN_IN.into());
    }
    // W1.2 (DEC-0027): an account the person turned off is not sent anywhere.
    if account.off.is_some() {
        return Err(switchboard_core::off::ACCOUNT_OFF.into());
    }
    if !account.usable() || snapshot.elsewhere.contains_key(id) {
        return Err(NOT_HERE.into());
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
    // SB-377 (operator 2026-11-14): a selected account moves too. Each group that has it selected
    // first moves to its next account here (`next_in_group`: one with room, else the one free
    // first), so the group's sessions go on; only a group with no other account refuses.
    for (key, value) in &snapshot.routes {
        if value != id {
            continue;
        }
        let group = key.split_once(':').map_or(key.as_str(), |(_, p)| p);
        match next_in_group(&snapshot, account, group, now) {
            Some(next) => store.select(account.provider, group, &next.id)?,
            None => return Err(SEND_ONLY_ONE.into()),
        }
    }
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
            return Err(e.into());
        }
    };
    let sent = credential.refresh_token.clone();
    let remove_there = |remote_id: &str| {
        run(
            &ssh(remote(
                program,
                // SB-403: a delegated copy is removed only under the key it was delegated under.
                &as_sender(
                    &["--json", "accounts", "remove", remote_id],
                    delegate.map(|d| d.sender_key),
                ),
            )),
            None,
        )
        .ok()
        .and_then(|out| data(&out))
        .is_some()
    };
    let hours = delegate.map(|d| d.lease_hours.to_string());
    let mut words = vec![
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
    ];
    if let (Some(d), Some(hours)) = (delegate, hours.as_deref()) {
        words.extend([
            "--delegated",
            "--sender-key",
            d.sender_key,
            "--lease-hours",
            hours,
        ]);
    }
    let added = run(
        &ssh(remote(program, &words)),
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
                    if new.is_empty() || (new.len() == 1 && remove_there(new[0])) {
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
            let Some(d) = delegate else {
                return Err(LEFT_THERE.into());
            };
            // Review round 2, P2-B: the seat there is recorded under its key, so `accounts recall`
            // finds it and takes the sign-in back; the account stays marked used there.
            // The print of what was sent goes with it (review round 3, P2-3): a sign-in renewed
            // here since is kept aside by the call back, never replaced by the spent copy there.
            let out = switchboard_core::DelegatedOut {
                remote_id: remote_id.clone(),
                lease_hours: d.lease_hours,
                sender_key: d.sender_key.to_owned(),
                sent: sent.as_deref().map(token_print),
            };
            if store.record_delegated_out(id, out.clone()).is_err() {
                crate::oplog::event(
                    "delegation_record",
                    &[("result", crate::oplog::Field::Code("unwritten"))],
                );
            }
            return Err(SendError {
                message: LEFT_THERE_DELEGATED.into(),
                left: Some(out),
            });
        }
        undo_here()?;
        return Err(if renewed_here {
            RENEWED_DURING
        } else {
            NOT_VERIFIED
        }
        .into());
    }
    // SB-323: the person's word on where the account belongs travels with it, so the copy there
    // serves the same organizations; the move stands whatever this step answers.
    let organizations = carry_organizations(&snapshot, account, &remote_id, &|args| {
        run(&ssh(remote(program, args)), None)
    });
    let mut moved = json!({
        "moved": true, "target": target, "remote_id": remote_id, "marked_elsewhere": true,
        "sent": sent.as_deref().map(token_print), "organizations": organizations,
    });
    if let Some(d) = delegate {
        moved["delegated"] = json!({"lease_hours": d.lease_hours});
    }
    Ok(moved)
}

/// SB-403: a delegated seat joins one of the other machine's groups, never its default group.
pub const DELEGATE_INTO_GROUP: &str = "A delegated seat joins one of the other machine's groups. Name that group; the default group takes none.";
/// SB-403: the other machine's restricted entry takes only delegated seats.
pub const DELEGATE_ONLY: &str = "That machine takes seats only as delegated ones. Move it again as a delegated seat (accounts send --delegate).";
/// SB-403: the seat's lease had run out there: its copy was removed, and the sign-in this machine
/// kept since the move did not answer here.
pub const LEASE_ENDED_SIGN_IN: &str = "The other machine removed the seat's sign-in 30 days after its lease ran out. The sign-in kept here did not answer; sign in to this account again.";
/// SB-403: a lease extension the other machine refused or did not answer; the read goes on.
pub const LEASE_NOT_EXTENDED: &str = "The other machine did not extend the seat's lease.";

/// SB-403: extends the lease of one delegated seat on the other machine to `hours` from now.
pub fn extend_lease(
    target: &str,
    remote_id: &str,
    hours: u32,
    sender_key: &str,
    program: &Program<'_>,
    run: &Run<'_>,
) -> Result<Value, String> {
    if !target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
    let hours = hours.to_string();
    let output = match run(
        &[
            target.to_owned(),
            "--".to_owned(),
            remote(
                program,
                &[
                    "--json",
                    "accounts",
                    "lease",
                    remote_id,
                    "--hours",
                    &hours,
                    "--sender-key",
                    sender_key,
                ],
            ),
        ],
        None,
    ) {
        Ok(out) | Err(out) => out,
    };
    data(&output).ok_or_else(|| refusal(&output).unwrap_or_else(|| LEASE_NOT_EXTENDED.into()))
}

/// SB-323: an organization the other machine could not take or place the account in.
pub const ORGANIZATION_NOT_CARRIED: &str = "The account moved, but the other machine did not place it in its organizations. Assign it there with switchboard org assign.";

/// SB-323: gives the copy on the other machine the person's word on this account's contexts
/// (its first one, `personal` included, then the organizations it also works for), adding an
/// organization the other machine lacks by its name and domains. Organizations match by name, as
/// a settings import matches them. Answers null when no word was said here (the rules decide
/// there as they do here), else `{carried, contexts}` and, when not carried, `reason`.
pub(crate) fn carry_organizations(
    snapshot: &switchboard_core::Snapshot,
    account: &switchboard_core::Account,
    remote_id: &str,
    run: &dyn Fn(&[&str]) -> Result<String, String>,
) -> Value {
    let attribution = snapshot.attribution(account);
    if attribution.by != Some(switchboard_core::By::Person) {
        return Value::Null;
    }
    let named: Vec<&switchboard_core::Organization> = attribution
        .organizations()
        .into_iter()
        .filter_map(|id| snapshot.organization(id))
        .collect();
    let contexts: Vec<String> = attribution
        .contexts()
        .into_iter()
        .filter_map(|c| match c {
            None => Some(switchboard_core::organizations::PERSONAL.to_owned()),
            Some(id) => snapshot.organization(id).map(|o| o.name.clone()),
        })
        .collect();
    let refused = |contexts: &[String]| json!({"carried": false, "contexts": contexts, "reason": ORGANIZATION_NOT_CARRIED});
    if !named.is_empty() {
        let Some(there) = run(&["--json", "org", "list"]).ok().and_then(|o| data(&o)) else {
            return refused(&contexts);
        };
        let has = |name: &str| {
            there["organizations"].as_array().is_some_and(|list| {
                list.iter().any(|o| {
                    o["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
            })
        };
        for o in named.iter().filter(|o| !has(&o.name)) {
            let mut args = vec!["--json", "org", "add", o.name.as_str()];
            for d in &o.domains {
                args.extend(["--domain", d.as_str()]);
            }
            if run(&args).ok().and_then(|out| data(&out)).is_none() {
                return refused(&contexts);
            }
        }
    }
    let mut args = vec!["--json", "org", "assign", remote_id];
    args.extend(contexts.iter().map(String::as_str));
    match run(&args).ok().and_then(|out| data(&out)) {
        Some(_) => json!({"carried": true, "contexts": contexts}),
        None => refused(&contexts),
    }
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
    // SB-403: a seat delegated here goes back only to the machine that delegated it, through
    // this machine's restricted entry (`surrender_delegated`).
    if store.snapshot()?.is_delegated(id) {
        return Err(switchboard_core::delegation::DELEGATED_NOT_SENT.into());
    }
    if undo {
        // SB-312: only the mark a surrender set is taken back, never one a move or a person set.
        let snapshot = store.snapshot()?;
        if !snapshot.elsewhere.contains_key(id) {
            return Ok(json!({"restored": false}));
        }
        let was_off = match snapshot.locations.get(id).map(String::as_str) {
            Some(CALLED_BACK) => false,
            Some(CALLED_BACK_OFF) => true,
            _ => return Err(NOT_CALLED_BACK.into()),
        };
        restore(store, id, was_off, now)?;
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
    // SB-377: a copy turned off here is given back too: nothing here uses it, so it is the
    // safest one to give, and an undo turns it off again.
    if snapshot.elsewhere.contains_key(id) {
        return Err(NOT_HERE.into());
    }
    let was_off = !account.enabled;
    if store.is_in_use(id) {
        return Err(IN_USE_HERE.into());
    }
    // SB-312: a group's selected account is the normal state after a move (switching is on by
    // default), so its group moves to another account of its own here instead of refusing.
    // SB-386 (operator 2026-11-15: accounts could not be called back from the Mac Studio and the
    // Mac Pro): a group with no other account here is left with none selected; the person asked
    // for the account back, and its mark below drops the selection (`mark_elsewhere_at`).
    let mut left_without = Vec::new();
    for (key, value) in &snapshot.routes {
        if value != id {
            continue;
        }
        let pool = key.split_once(':').map_or(key.as_str(), |(_, p)| p);
        // SB-377: the group's best next account (room first, else the one free first), not the
        // first one listed.
        match next_in_group(&snapshot, account, pool, now) {
            Some(next) => store.select(account.provider, pool, &next.id)?,
            None => left_without.push(pool.to_owned()),
        }
    }
    let mark = if was_off {
        CALLED_BACK_OFF
    } else {
        CALLED_BACK
    };
    store.mark_elsewhere_at(id, true, now, Some(mark))?;
    let credential = match store.stored_credential(id) {
        Ok(c) => c,
        Err(e) => {
            restore(store, id, was_off, now)?;
            return Err(e);
        }
    };
    if let Some(token) = credential.refresh_token.as_deref() {
        settle(token);
    }
    let credential = store.stored_credential(id)?;
    let tokens: Value =
        serde_json::from_str(&credential.to_oauth_json(account.provider)).unwrap_or(Value::Null);
    // W1.2 (DEC-0027): the person's off goes with the sign-in, so the copy that comes home stays
    // off (`recall_take_as`); null when it was on. An older Switchboard sends no such field.
    Ok(
        json!({"provider": account.provider.as_str(), "credential": tokens,
        "groups_left_without_account": left_without, "off_since": account.off_since()}),
    )
}

/// SB-403 (DEC-0025), on the machine a seat was delegated to, through `ssh-serve`: gives the
/// seat's sign-in back to the machine whose key delegated it. The seat is switched off first, so
/// this machine never renews it again; a seat whose lease ran out answers `lease_ended` and no
/// sign-in. `undo` switches it on again when the caller could not take it.
pub fn surrender_delegated(
    store: &Store,
    id: &str,
    sender_key: &str,
    undo: bool,
    settle: &dyn Fn(&str),
) -> Result<Value, String> {
    if undo {
        store.surrender_delegated(id, sender_key, true)?;
        return Ok(json!({"restored": true}));
    }
    let provider = store
        .snapshot()?
        .accounts
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.provider)
        .ok_or(switchboard_core::delegation::NOT_DELEGATED_BY_YOU)?;
    let Some(credential) = store.surrender_delegated(id, sender_key, false)? else {
        return Ok(
            json!({"provider": provider.as_str(), "credential": null, "lease_ended": true,
            "groups_left_without_account": []}),
        );
    };
    if let Some(token) = credential.refresh_token.as_deref() {
        settle(token);
    }
    // A renewal that was on the network when it was switched off has stored its successor.
    let credential = store.stored_credential(id)?;
    let tokens: Value =
        serde_json::from_str(&credential.to_oauth_json(provider)).unwrap_or(Value::Null);
    Ok(json!({"provider": provider.as_str(), "credential": tokens,
        "groups_left_without_account": []}))
}

/// Takes a surrender's mark back: used here again, and off again when it was off.
fn restore(store: &Store, id: &str, was_off: bool, now: i64) -> Result<(), String> {
    store.mark_elsewhere(id, false, now)?;
    if was_off {
        store.update_fields(id, None, Some(false))?;
        return Ok(());
    }
    // SB-386: a group the surrender left with no account selected takes this one again.
    let snapshot = store.snapshot()?;
    if let Some(account) = snapshot.accounts.iter().find(|a| a.id == id) {
        let key = format!("{}:{}", account.provider.as_str(), account.pool);
        if !snapshot.routes.contains_key(&key) {
            let _ = store.select(account.provider, &account.pool, id);
        }
    }
    Ok(())
}

/// What `recall_take` brought back, for `recall_undo` when the provider does not answer it here.
pub struct Taken {
    pub remote_id: String,
    /// The refresh token taken back, to tell a renewal here from no answer.
    pub token: Option<String>,
    /// SB-377: the sign-in this machine held when it changed after the move (its print no longer
    /// the one sent), kept aside: when the copy brought back does not answer, this one is tried.
    pub kept: Option<Credential>,
    /// SB-403: the other machine held a delegated seat whose lease had run out: it had removed
    /// its copy, so nothing came back and the sign-in kept here is the one asked.
    pub lease_ended: bool,
    /// SB-403: the key that delegated the seat there (this machine's), named in every call that
    /// acts on it there; None for a plain move.
    pub sender_key: Option<String>,
}

/// SB-292 (S-3), on this machine: take the sign-in of an account this machine moved away
/// (`send`) back from `target`. The other machine marks it used elsewhere and hands its tokens
/// over ssh; they replace the spent ones here (`Store::adopt_refreshed`, identity checked) and
/// the mark here is cleared. The caller checks the provider answers, then `recall_finish`.
#[allow(clippy::too_many_arguments)]
pub fn recall_take(
    store: &Store,
    id: &str,
    target: &str,
    remote_id: &str,
    program: &Program<'_>,
    now: i64,
    run: &Run<'_>,
    sent: Option<&str>,
) -> Result<Taken, String> {
    recall_take_as(store, id, target, remote_id, program, now, run, sent, None)
}
/// The words that name a delegated seat's sender to the other machine (SB-403): its restricted
/// entry acts on a seat only for the key that delegated it.
fn as_sender<'a>(words: &[&'a str], sender_key: Option<&'a str>) -> Vec<&'a str> {
    let mut words = words.to_vec();
    if let Some(key) = sender_key {
        words.extend(["--sender-key", key]);
    }
    words
}
/// [`recall_take`] of a seat this machine delegated (`sender_key`, SB-403); None as before.
#[allow(clippy::too_many_arguments)]
pub fn recall_take_as(
    store: &Store,
    id: &str,
    target: &str,
    remote_id: &str,
    program: &Program<'_>,
    now: i64,
    run: &Run<'_>,
    sent: Option<&str>,
    sender_key: Option<&str>,
) -> Result<Taken, String> {
    if !target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
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
    // SB-315: a Codex sign-in comes back as a Claude one does (its tokens round-trip,
    // `an_exported_sign_in_reads_back_the_same_and_carries_nothing_else`).
    let provider = account.provider;
    if !snapshot.elsewhere.contains_key(id) {
        return Err(NOT_AWAY.into());
    }
    let spent = store
        .stored_credential(id)?
        .refresh_token
        .ok_or(NOT_A_SIGN_IN)?;
    // SB-312, SB-377: the sign-in here changed after it left (a renewal, a sync of the ordinary
    // Claude Code). Before, the call back was refused and no account could come back; now the
    // copy there is taken, and the one here is kept aside: whichever the provider answers stays
    // (`Taken::kept`, tried by the caller when the copy brought back does not answer).
    let kept = if sent.is_some_and(|print| token_print(&spent) != print) {
        Some(store.stored_credential(id)?)
    } else {
        None
    };
    let ssh = |command: String| vec![target.to_owned(), "--".to_owned(), command];
    let undo_there = || {
        let _ = run(
            &ssh(remote(
                program,
                &as_sender(
                    &["--json", "accounts", "surrender", remote_id, "--undo"],
                    sender_key,
                ),
            )),
            None,
        );
    };
    // A delegated seat comes back through the other machine's restricted entry (`ssh-serve`),
    // which reads the words itself; a moved account through its `accounts surrender`.
    let surrender = as_sender(&["--json", "accounts", "surrender", remote_id], sender_key);
    let answer = run(
        &ssh(if sender_key.is_some() {
            remote(program, &surrender)
        } else {
            remote_transfer(program, &surrender)
        }),
        None,
    );
    let output = match &answer {
        Ok(out) | Err(out) => out.as_str(),
    };
    let Some(given) = data(output) else {
        // SB-312: the other machine's own reason, when it gave one; a lost answer may have
        // marked it there, so that mark is taken back.
        if let Some(reason) = refusal(output) {
            // SB-386: "on this machine" in an older Switchboard's answer meant that machine.
            if reason == ONLY_ONE_HERE {
                return Err(OLD_THERE.into());
            }
            return Err(reason);
        }
        undo_there();
        return Err(SURRENDER_FAILED.into());
    };
    // SB-403: a delegated seat whose lease ran out there holds no sign-in any more; the one kept
    // here since the move is the one asked.
    if given["lease_ended"] == true {
        store.mark_elsewhere(id, false, now)?;
        return Ok(Taken {
            remote_id: remote_id.to_owned(),
            token: Some(spent),
            kept: None,
            lease_ended: true,
            sender_key: sender_key.map(str::to_owned),
        });
    }
    let tokens = given["credential"].to_string();
    let Ok(credential) = Credential::parse(provider, AuthKind::OAuth, &tokens) else {
        undo_there();
        return Err(SURRENDER_FAILED.into());
    };
    if store
        .adopt_refreshed(provider, &spent, &credential)
        .is_err()
    {
        undo_there();
        return Err(SURRENDER_FAILED.into());
    }
    // W1.2 (DEC-0027): a copy the person turned off there comes home off, with its time, in
    // place before the mark is cleared (`mark_elsewhere` leaves an account that is off off). A
    // delegated seat's off is the teammate's word about their own use, not carried.
    if sender_key.is_none() {
        if let Some(since) = given["off_since"].as_i64().filter(|s| *s > 0) {
            store.turn_off_since(id, since)?;
        }
    }
    store.mark_elsewhere(id, false, now)?;
    Ok(Taken {
        remote_id: remote_id.to_owned(),
        token: credential.refresh_token.clone(),
        kept,
        lease_ended: false,
        sender_key: sender_key.map(str::to_owned),
    })
}

/// The provider did not answer the sign-in here: it goes back to the other machine, which holds
/// the same tokens and renews them again, and this machine stops using it.
pub fn recall_undo(
    store: &Store,
    id: &str,
    target: &str,
    taken: &Taken,
    program: &Program<'_>,
    now: i64,
    run: &Run<'_>,
) -> Result<(), String> {
    store.mark_elsewhere_at(id, true, now, Some(target))?;
    let command = remote(
        program,
        &as_sender(
            &[
                "--json",
                "accounts",
                "surrender",
                &taken.remote_id,
                "--undo",
            ],
            taken.sender_key.as_deref(),
        ),
    );
    let _ = run(&[target.to_owned(), "--".to_owned(), command], None);
    Ok(())
}

/// SB-292 (S-4): the sign-in works here; remove the other machine's copy and prove it is gone:
/// its account list no longer names it. Answers `{recalled, removed_there, proved}`.
pub fn recall_finish(target: &str, taken: &Taken, program: &Program<'_>, run: &Run<'_>) -> Value {
    let ssh = |command: String| vec![target.to_owned(), "--".to_owned(), command];
    let removed = run(
        &ssh(remote(
            program,
            &as_sender(
                &["--json", "accounts", "remove", &taken.remote_id],
                taken.sender_key.as_deref(),
            ),
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
    ssh_runner_for(args, stdin, std::time::Duration::from_secs(120))
}
/// [`ssh_runner`] given up after `limit` (SB-355: a live read of a machine group waits 20 s).
pub fn ssh_runner_for(
    args: &[String],
    stdin: Option<&[u8]>,
    limit: std::time::Duration,
) -> Result<String, String> {
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
    let deadline = std::time::Instant::now() + limit;
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
    fn a_named_data_folder_goes_to_every_command_there() {
        // SB-315: a Switchboard on the other machine that runs with --data-dir.
        let program = Program {
            path: "/usr/local/bin/switchboard",
            data_dir: Some("/srv/switch board"),
        };
        assert_eq!(
            remote(&program, &["--json", "accounts", "list"]),
            "'/usr/local/bin/switchboard' '--data-dir' '/srv/switch board' '--json' 'accounts' 'list'"
        );
        assert_eq!(
            remote(&DEFAULT_APP, &["--json"]),
            format!("'{REMOTE_DEFAULT}' '--json'")
        );
        let relative = Program {
            data_dir: Some("data"),
            ..program
        };
        assert_eq!(relative.check().unwrap_err(), REMOTE_DATA_DIR_INVALID);
        let bare = Program {
            path: "switchboard",
            data_dir: None,
        };
        assert_eq!(bare.check().unwrap_err(), REMOTE_INVALID);
        assert!(program.check().is_ok());
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
            &DEFAULT_APP,
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
    fn the_persons_word_on_its_organizations_goes_with_the_account() {
        // SB-323: an account the person placed here (personal, also Acme) arrives there in the
        // same contexts; an organization the other machine lacks is added by name and domains.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let acme = store
            .add_organization("Acme", &["acme.test".to_owned()])
            .unwrap();
        store
            .assign_organizations(&id, &["personal".to_owned(), acme.id.clone()])
            .unwrap();
        let calls: Calls = Default::default();
        let answer = send(
            &store,
            &id,
            "studio.local",
            "agents",
            &DEFAULT_APP,
            100,
            &fake(&calls, true, true),
            &|_| {},
        )
        .unwrap();
        assert_eq!(answer["organizations"]["carried"], true);
        assert_eq!(
            answer["organizations"]["contexts"],
            json!(["personal", "Acme"])
        );
        let calls = calls.lock().unwrap();
        let commands: Vec<&str> = calls.iter().map(|(c, _)| c.as_str()).collect();
        assert!(commands[4].ends_with("'--json' 'org' 'list'"));
        assert!(commands[5].ends_with("'--json' 'org' 'add' 'Acme' '--domain' 'acme.test'"));
        assert!(commands[6].ends_with("'--json' 'org' 'assign' 'remote-1' 'personal' 'Acme'"));
    }

    #[test]
    fn an_organization_the_other_machine_holds_is_not_added_again_and_a_refusal_keeps_the_move() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let acme = store.add_organization("Acme", &[]).unwrap();
        store.assign_organization(&id, Some(&acme.id)).unwrap();
        let calls: Calls = Default::default();
        let base = fake(&calls, true, true);
        let refusing = |args: &[String], stdin: Option<&[u8]>| {
            if args[2].contains("'org' 'list'") {
                return Ok(r#"{"ok":true,"data":{"organizations":[{"name":"acme"}]}}"#.into());
            }
            if args[2].contains("'org' 'assign'") {
                return Err(r#"{"ok":false,"error":"Account not found"}"#.into());
            }
            base(args, stdin)
        };
        let answer = send(
            &store,
            &id,
            "studio.local",
            "agents",
            &DEFAULT_APP,
            100,
            &refusing,
            &|_| {},
        )
        .unwrap();
        assert_eq!(answer["moved"], true);
        assert_eq!(answer["organizations"]["carried"], false);
        assert_eq!(answer["organizations"]["reason"], ORGANIZATION_NOT_CARRIED);
        assert!(!calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c.contains("'org' 'add'")));
        assert!(store.snapshot().unwrap().elsewhere.contains_key(&id));
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
                &DEFAULT_APP,
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

    fn observed(store: &Store, id: &str, used: f64, resets: i64) {
        store
            .observe(
                id,
                switchboard_core::Usage {
                    used_percent: used,
                    observed_at: 90,
                    resets_at: Some(resets),
                    source: "claude_oauth".into(),
                    windows: vec![switchboard_core::UsageWindow {
                        name: "five_hour".into(),
                        used_percent: used,
                        resets_at: Some(resets),
                    }],
                },
            )
            .unwrap();
    }
    fn another(store: &Store, name: &str) -> String {
        store
            .add(
                name.into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                Credential {
                    access_token: format!("{name}-access"),
                    refresh_token: Some(format!("{name}-refresh")),
                    id_token: None,
                    native_context: None,
                    expires_at: Some(2_000_000_000),
                    account_id: None,
                },
            )
            .unwrap()
            .id
    }

    #[test]
    fn a_selected_account_moves_after_its_group_takes_its_best_next_account() {
        // SB-377 (operator 2026-11-14): before, a selected account refused to move ("Select
        // another account for its group"). Now its group moves first to the account with room,
        // and only a group with no other account here refuses.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store.select(Provider::Claude, "default", &id).unwrap();
        let calls: Calls = Default::default();
        assert_eq!(
            send(
                &store,
                &id,
                "studio.local",
                "default",
                &DEFAULT_APP,
                100,
                &fake(&calls, true, true),
                &|_| {}
            )
            .unwrap_err(),
            SEND_ONLY_ONE
        );
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        let full = another(&store, "full");
        let room = another(&store, "room");
        observed(&store, &full, 100.0, 5_000);
        observed(&store, &room, 30.0, 5_000);
        send(
            &store,
            &id,
            "studio.local",
            "default",
            &DEFAULT_APP,
            100,
            &fake(&calls, true, true),
            &|_| {},
        )
        .unwrap();
        let snapshot = store.snapshot().unwrap();
        assert_eq!(snapshot.routes.get("claude:default"), Some(&room));
        assert!(snapshot.elsewhere.contains_key(&id));
    }

    #[test]
    fn the_next_account_is_the_one_with_room_else_the_one_free_first() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let late = another(&store, "late");
        let soon = another(&store, "soon");
        observed(&store, &late, 100.0, 9_000);
        observed(&store, &soon, 100.0, 3_000);
        let snapshot = store.snapshot().unwrap();
        let me = snapshot.accounts.iter().find(|a| a.id == id).unwrap();
        // Every other account is full: the one whose limit resets first.
        assert_eq!(
            next_in_group(&snapshot, me, "default", 100).map(|a| a.id.clone()),
            Some(soon.clone())
        );
        // One never measured comes before a full one, and one with room before both.
        let never = another(&store, "never");
        let snapshot = store.snapshot().unwrap();
        let me = snapshot.accounts.iter().find(|a| a.id == id).unwrap();
        assert_eq!(
            next_in_group(&snapshot, me, "default", 100).map(|a| a.id.clone()),
            Some(never)
        );
        let room = another(&store, "room");
        observed(&store, &room, 60.0, 9_000);
        let snapshot = store.snapshot().unwrap();
        let me = snapshot.accounts.iter().find(|a| a.id == id).unwrap();
        assert_eq!(
            next_in_group(&snapshot, me, "default", 100).map(|a| a.id.clone()),
            Some(room)
        );
    }

    #[test]
    fn claude_code_takes_a_sign_in_whose_identity_is_known_and_that_has_room() {
        // SB-377: before the account Claude Code is signed in to moves, Claude Code takes the
        // next one; only a sign-in whose identity is known can become Claude Code's.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let blind = another(&store, "blind");
        observed(&store, &blind, 10.0, 9_000);
        let snapshot = store.snapshot().unwrap();
        let me = snapshot.accounts.iter().find(|a| a.id == id).unwrap();
        assert!(next_for_cli(&snapshot, me, 100).is_none());
        let identity = |n: &str| switchboard_core::ExternalIdentity {
            account_id: Some(format!("{n}-uuid")),
            organization_id: None,
            email: Some(format!("{n}@example.com")),
        };
        let full = another(&store, "full");
        let room = another(&store, "room");
        store.learn_identity(&full, &identity("full")).unwrap();
        store.learn_identity(&room, &identity("room")).unwrap();
        observed(&store, &full, 100.0, 3_000);
        observed(&store, &room, 70.0, 9_000);
        let snapshot = store.snapshot().unwrap();
        let me = snapshot.accounts.iter().find(|a| a.id == id).unwrap();
        assert_eq!(
            next_for_cli(&snapshot, me, 100).map(|a| a.id.clone()),
            Some(room)
        );
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
                &DEFAULT_APP,
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
                &DEFAULT_APP,
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
                    &DEFAULT_APP,
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
                &Program {
                    path: "switchboard",
                    data_dir: None,
                },
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
            // SB-420: the call that hands a sign-in over names the hand-over in its command.
            if command.contains("'surrender'") {
                assert!(
                    command.starts_with("/usr/bin/env SWITCHBOARD_TRANSFER=surrender '"),
                    "{command}"
                );
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
                &DEFAULT_APP,
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
                &DEFAULT_APP,
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
                &DEFAULT_APP,
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
            &DEFAULT_APP,
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
            &DEFAULT_APP,
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
            &DEFAULT_APP,
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
            &DEFAULT_APP,
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
            &DEFAULT_APP,
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
        // SB-386: the only account of its group is given back too; the group is left with none
        // selected and says so, and an undo selects it again.
        let given = surrender(&store, &id, false, 7, &|_| {}).unwrap();
        assert_eq!(given["groups_left_without_account"], json!(["default"]));
        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.elsewhere.contains_key(&id));
        assert_eq!(snapshot.routes.get("claude:default"), None);
        surrender(&store, &id, true, 7, &|_| {}).unwrap();
        let snapshot = store.snapshot().unwrap();
        assert!(!snapshot.elsewhere.contains_key(&id));
        assert_eq!(snapshot.routes.get("claude:default"), Some(&id));
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
    fn a_sign_in_changed_here_after_it_moved_is_called_back_with_the_one_here_kept_aside() {
        // SB-377: before, a print that no longer matched refused every call back ("renewed here
        // after it moved"), so no account could come back. Now the copy there is taken and the
        // sign-in here is kept aside for the caller to try when the copy does not answer.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store
            .mark_elsewhere_at(&id, true, 1, Some("studio.local"))
            .unwrap();
        let before = store.stored_credential(&id).unwrap();
        let calls: Calls = Default::default();
        let print = token_print("an-older-token");
        let taken = recall_take(
            &store,
            &id,
            "studio.local",
            "remote-1",
            &DEFAULT_APP,
            2,
            &member(&calls, Some(RENEWED), false),
            Some(&print),
        )
        .unwrap();
        assert_eq!(
            store
                .stored_credential(&id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-refresh-2"),
            "the copy brought back is tried first"
        );
        let kept = taken.kept.expect("the sign-in here is kept aside");
        assert_eq!(kept.refresh_token, before.refresh_token);
        // It can be put back: the caller does this when the copy brought back does not answer.
        store
            .adopt_refreshed(Provider::Claude, "synthetic-refresh-2", &kept)
            .unwrap();
        assert_eq!(
            store.stored_credential(&id).unwrap().refresh_token,
            before.refresh_token
        );
        // The print that matches keeps nothing aside, as before.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store
            .mark_elsewhere_at(&id, true, 1, Some("studio.local"))
            .unwrap();
        let same = token_print(&store.stored_credential(&id).unwrap().refresh_token.unwrap());
        let taken = recall_take(
            &store,
            &id,
            "studio.local",
            "remote-1",
            &DEFAULT_APP,
            2,
            &member(&calls, Some(RENEWED), false),
            Some(&same),
        )
        .unwrap();
        assert!(taken.kept.is_none());
        assert_eq!(token_print("synthetic-refresh").len(), 16);
    }

    /// SB-315: a Codex sign-in is called back as a Claude one is: the other machine gives it
    /// back as `codex` and it replaces the spent copy here.
    #[test]
    fn a_codex_sign_in_comes_back_too() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let codex = |refresh: &str| Credential {
            access_token: format!("access-{refresh}"),
            refresh_token: Some(refresh.into()),
            id_token: Some("synthetic-id".into()),
            native_context: None,
            expires_at: Some(2_000_000_000),
            account_id: Some("synthetic-workspace".into()),
        };
        let id = store
            .add(
                "Codex seat".into(),
                Provider::Codex,
                AuthKind::OAuth,
                "default".into(),
                codex("codex-refresh"),
            )
            .unwrap()
            .id;
        // Its own side: the surrender answers `codex`.
        let given = surrender(&store, &id, false, 3, &|_| {}).unwrap();
        assert_eq!(given["provider"], "codex");
        surrender(&store, &id, true, 4, &|_| {}).unwrap();
        // The calling side takes back what the other machine renewed meanwhile.
        store
            .mark_elsewhere_at(&id, true, 5, Some("studio.local"))
            .unwrap();
        let renewed = codex("codex-refresh-2").to_oauth_json(Provider::Codex);
        let leaked: &'static str = Box::leak(renewed.into_boxed_str());
        let calls: Calls = Default::default();
        assert!(recall_take(
            &store,
            &id,
            "studio.local",
            "remote-1",
            &DEFAULT_APP,
            6,
            &member(&calls, Some(leaked), false),
            None,
        )
        .is_ok());
        let back = store.stored_credential(&id).unwrap();
        assert_eq!(back.refresh_token.as_deref(), Some("codex-refresh-2"));
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
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

    #[test]
    fn a_copy_turned_off_here_is_given_back_and_an_undo_turns_it_off_again() {
        // SB-377 (2026-11-15, the Mac Studio): claude6 was off there, nothing used it, and the
        // call back from the MacBook was refused with NOT_HERE.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store.update(&id, "Work O'Neil".into(), false).unwrap();
        let given = surrender(&store, &id, false, 7, &|_| {}).unwrap();
        assert_eq!(given["provider"], "claude");
        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.elsewhere.contains_key(&id));
        assert_eq!(
            snapshot.locations.get(&id).map(String::as_str),
            Some(CALLED_BACK_OFF)
        );
        surrender(&store, &id, true, 9, &|_| {}).unwrap();
        let snapshot = store.snapshot().unwrap();
        assert!(!snapshot.elsewhere.contains_key(&id));
        assert!(snapshot.accounts.iter().any(|a| a.id == id && !a.enabled));
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
                &DEFAULT_APP,
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
            &DEFAULT_APP,
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
            &DEFAULT_APP,
            &member(&calls, None, false),
        );
        assert_eq!(done["removed_there"], true);
        assert_eq!(done["proved"], true);
        let kept = recall_finish(
            "studio.local",
            &taken,
            &DEFAULT_APP,
            &member(&calls, None, true),
        );
        assert_eq!(
            kept["proved"], false,
            "a copy still listed there is not proved gone"
        );
    }

    /// W1.2 (DEC-0027), the leak: a copy the person turned off on the other machine came home
    /// on. Its surrender answer now says since when it is off, and it comes home off.
    #[test]
    fn a_copy_turned_off_there_comes_home_off_with_its_time() {
        // The other machine's side: turned off there, given back with its time.
        let (_there_root, there, there_id) = store_with(AuthKind::OAuth);
        there.update_fields(&there_id, None, Some(false)).unwrap();
        let since = there.snapshot().unwrap().accounts[0].off_since().unwrap();
        let given = surrender(&there, &there_id, false, 7, &|_| {}).unwrap();
        assert_eq!(given["off_since"], since);
        // This machine calls it back.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store
            .mark_elsewhere_at(&id, true, 5, Some("studio.local"))
            .unwrap();
        let answer = json!({"ok": true, "data": {"provider": "claude",
            "credential": serde_json::from_str::<Value>(RENEWED).unwrap(), "off_since": since}})
        .to_string();
        let run = move |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            assert!(args[2].contains("'surrender'"));
            Ok(answer.clone())
        };
        recall_take(
            &store,
            &id,
            "studio.local",
            "remote-1",
            &DEFAULT_APP,
            6,
            &run,
            None,
        )
        .unwrap();
        let s = store.snapshot().unwrap();
        let a = s.accounts.iter().find(|a| a.id == id).unwrap();
        assert!(!s.elsewhere.contains_key(&id), "back here");
        assert!(
            !a.enabled && a.off_since() == Some(since),
            "and off, as there"
        );
    }

    #[test]
    fn an_older_machines_only_account_refusal_says_to_update_it_there() {
        // SB-386: before 0.7.94 the other machine refused an account its group used alone, in
        // words that read as if this machine refused it.
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store
            .mark_elsewhere_at(&id, true, 5, Some("studio.local"))
            .unwrap();
        let refused = json!({"ok": false, "error": ONLY_ONE_HERE}).to_string();
        let run = move |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err(refused.clone())
        };
        assert_eq!(
            recall_take(
                &store,
                &id,
                "studio.local",
                "remote-1",
                &DEFAULT_APP,
                6,
                &run,
                None
            )
            .err()
            .unwrap(),
            OLD_THERE
        );
        assert!(
            store.snapshot().unwrap().elsewhere.contains_key(&id),
            "nothing changed here"
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
                &DEFAULT_APP,
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
            &DEFAULT_APP,
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
            &DEFAULT_APP,
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
            &DEFAULT_APP,
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
                &DEFAULT_APP,
                9,
                &member(&calls, Some(RENEWED), false),
                None
            )
            .err()
            .unwrap(),
            NO_REMOTE_ID
        );
    }

    /// SB-403: the words a fake ssh was given, unquoted (no word here holds a quote).
    fn unquoted(command: &str) -> Vec<String> {
        command
            .trim_matches('\'')
            .split("' '")
            .map(str::to_owned)
            .collect()
    }
    fn after<'a>(words: &'a [String], flag: &str) -> Option<&'a str> {
        let at = words.iter().position(|w| w == flag)?;
        words.get(at + 1).map(String::as_str)
    }

    /// SB-403 (DEC-0025): a delegated seat goes to the other machine as a guest pinned to this
    /// machine's key, and comes back only to that key. The other machine is a second store, which
    /// the fake ssh drives as that machine's `ssh-serve` would.
    #[test]
    fn a_delegated_seat_goes_there_as_a_guest_and_comes_back_only_to_its_sender() {
        const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let there_root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let there = Store::open(
            there_root.path().to_owned(),
            Arc::new(MemoryVault::default()),
        )
        .unwrap();
        let folder = std::fs::canonicalize(folder.path()).unwrap();
        let group = there.save_project(None, "Org", &[folder], &[]).unwrap();
        let calls: Calls = Default::default();
        let run = |args: &[String], stdin: Option<&[u8]>| -> Result<String, String> {
            let words = unquoted(&args[2]);
            calls.lock().unwrap().push((args[2].clone(), None));
            let w: Vec<&str> = words.iter().skip(2).map(String::as_str).collect();
            let answer = |data: Value| Ok(json!({"ok": true, "data": data}).to_string());
            let refused = |e: String| Err(json!({"ok": false, "error": e}).to_string());
            match w.as_slice() {
                ["project", "list"] => answer(json!({"projects": [{"pool": group.pool}]})),
                ["accounts", "list"] => answer(json!({"accounts": there
                    .snapshot()
                    .unwrap()
                    .accounts
                    .iter()
                    .map(|a| json!({"id": a.id}))
                    .collect::<Vec<_>>()})),
                ["accounts", "add", ..] => {
                    assert_eq!(after(&words, "--sender-key"), Some(KEY));
                    assert!(words.iter().any(|w| w == "--delegated"));
                    let hours: u32 = after(&words, "--lease-hours").unwrap().parse().unwrap();
                    let credential = Credential::parse(
                        Provider::Claude,
                        AuthKind::OAuth,
                        std::str::from_utf8(stdin.unwrap()).unwrap(),
                    )
                    .unwrap();
                    match there.add_delegated(
                        "seat".into(),
                        Provider::Claude,
                        after(&words, "--pool").unwrap().into(),
                        credential,
                        KEY,
                        None,
                        hours,
                    ) {
                        Ok(a) => answer(json!({"id": a.id})),
                        Err(e) => refused(e),
                    }
                }
                ["usage", _] => answer(json!({})),
                ["accounts", "surrender", remote, ..] => {
                    match surrender_delegated(
                        &there,
                        remote,
                        after(&words, "--sender-key").unwrap_or(""),
                        words.iter().any(|w| w == "--undo"),
                        &|_| {},
                    ) {
                        Ok(v) => answer(v),
                        Err(e) => refused(e),
                    }
                }
                ["accounts", "remove", remote, ..] => {
                    match there
                        .remove_delegated(remote, after(&words, "--sender-key").unwrap_or(""))
                    {
                        Ok(()) => answer(json!({"removed": true})),
                        Err(e) => refused(e),
                    }
                }
                _ => Ok(r#"{"ok":true,"data":null}"#.into()),
            }
        };
        let delegate = Delegate {
            sender_key: KEY,
            lease_hours: 48,
        };
        // Never into the other machine's default group.
        assert_eq!(
            send_as(
                &store,
                &id,
                "studio.local",
                "default",
                &DEFAULT_APP,
                100,
                &run,
                &|_| {},
                Some(delegate)
            )
            .unwrap_err(),
            DELEGATE_INTO_GROUP
        );
        let moved = send_as(
            &store,
            &id,
            "studio.local",
            &group.pool,
            &DEFAULT_APP,
            100,
            &run,
            &|_| {},
            Some(delegate),
        )
        .unwrap();
        assert_eq!(moved["delegated"]["lease_hours"], 48);
        let remote_id = moved["remote_id"].as_str().unwrap().to_owned();
        let guest = there.snapshot().unwrap();
        assert!(guest.is_delegated(&remote_id));
        // There, the guest is never moved on.
        assert_eq!(
            send(
                &there,
                &remote_id,
                "x.local",
                "agents",
                &DEFAULT_APP,
                100,
                &run,
                &|_| {}
            )
            .unwrap_err(),
            switchboard_core::delegation::DELEGATED_NOT_SENT
        );
        // A call back without the key is refused there; with it, the seat comes home.
        assert!(recall_take(
            &store,
            &id,
            "studio.local",
            &remote_id,
            &DEFAULT_APP,
            200,
            &run,
            None
        )
        .is_err());
        let taken = recall_take_as(
            &store,
            &id,
            "studio.local",
            &remote_id,
            &DEFAULT_APP,
            200,
            &run,
            None,
            Some(KEY),
        )
        .unwrap();
        assert!(!taken.lease_ended);
        assert!(
            !there.snapshot().unwrap().accounts[0].enabled,
            "switched off there"
        );
        let done = recall_finish("studio.local", &taken, &DEFAULT_APP, &run);
        assert_eq!(done["proved"], true);
        assert!(there.snapshot().unwrap().accounts.is_empty());
        assert!(
            calls
                .lock()
                .unwrap()
                .iter()
                .any(|(c, _)| c
                    .contains(&format!("'surrender' '{remote_id}' '--sender-key' '{KEY}'")))
        );
        // The restricted entry reads the words itself: a call under the key has no variable in
        // front of them (SB-420); the call back without the key above had one.
        let calls = calls.lock().unwrap();
        assert!(calls
            .iter()
            .filter(|(c, _)| c.contains("'--sender-key'"))
            .all(|(c, _)| !c.contains("SWITCHBOARD_TRANSFER")));
        assert!(calls.iter().any(|(c, _)| c
            .starts_with("/usr/bin/env SWITCHBOARD_TRANSFER=surrender '")
            && c.contains(&format!("'surrender' '{remote_id}'"))));
    }

    /// SB-403 (review P1-2): a delegated move whose check fails there removes the copy there
    /// under the key it was delegated under (the restricted entry refuses a removal without it),
    /// so no guest is left running there for its lease.
    #[test]
    fn a_delegated_move_that_fails_its_check_removes_the_copy_there_under_its_key() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        let err = send_as(
            &store,
            &id,
            "studio.local",
            "agents",
            &DEFAULT_APP,
            100,
            &fake(&calls, true, false),
            &|_| {},
            Some(Delegate {
                sender_key: "K",
                lease_hours: 48,
            }),
        )
        .unwrap_err();
        assert_eq!(err, NOT_VERIFIED);
        assert!(calls
            .lock()
            .unwrap()
            .iter()
            .any(|(c, _)| c.contains("'accounts' 'remove' 'remote-1' '--sender-key' 'K'")));
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        // Review round 2, P2-B: when the removal there fails too, the seat is recorded under its
        // key, so a call back finds it and takes the sign-in back.
        const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
        let calls: Calls = Default::default();
        let failing = |args: &[String], stdin: Option<&[u8]>| {
            if args[2].contains("'accounts' 'remove'") {
                return Err("ssh failed.".to_string());
            }
            if args[2].contains("'accounts' 'add'") {
                return Ok(r#"{"ok":true,"data":{"id":"0a1b2c3d"}}"#.to_string());
            }
            fake(&calls, true, false)(args, stdin)
        };
        assert_eq!(
            send_as(
                &store,
                &id,
                "studio.local",
                "agents",
                &DEFAULT_APP,
                100,
                &failing,
                &|_| {},
                Some(Delegate {
                    sender_key: KEY,
                    lease_hours: 48,
                }),
            )
            .unwrap_err(),
            LEFT_THERE_DELEGATED
        );
        assert!(LEFT_THERE_DELEGATED.contains("accounts recall"));
        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.elsewhere.contains_key(&id));
        let out = &snapshot.delegated_out[&id];
        assert_eq!(out.remote_id, "0a1b2c3d");
        assert_eq!(out.lease_hours, 48);
        assert_eq!(out.sender_key, KEY);
        assert_eq!(
            out.sent.as_deref(),
            Some(token_print("synthetic-refresh").as_str())
        );
    }

    /// Review round 3: a delegated copy left there after this machine renewed the sign-in while
    /// it moved (P2-3) carries the print of the token sent, so the call back keeps the live
    /// sign-in renewed here aside instead of losing it to the spent copy there; taking it back
    /// clears the record (P2-1).
    #[test]
    fn a_copy_left_there_after_a_renewal_here_is_called_back_keeping_the_live_sign_in() {
        const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
        let (_root, store, id) = store_with(AuthKind::OAuth);
        let calls: Calls = Default::default();
        let spent = Credential {
            access_token: "synthetic-access".into(),
            refresh_token: Some("synthetic-refresh".into()),
            id_token: None,
            native_context: None,
            expires_at: Some(2_000_000_000),
            account_id: Some("synthetic-account".into()),
        };
        let renewing = studio(&calls, None, |command| {
            if command.contains("'accounts' 'add'") {
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
                return Ok(r#"{"ok":true,"data":{"id":"0a1b2c3d"}}"#.into());
            }
            if command.contains("'accounts' 'remove'") {
                return Err("ssh failed.".into());
            }
            Ok(r#"{"ok":true,"data":{}}"#.into())
        });
        let err = send_with(
            &store,
            &id,
            "studio.local",
            "agents",
            &DEFAULT_APP,
            1,
            &renewing,
            &|_| {},
            Some(Delegate {
                sender_key: KEY,
                lease_hours: 48,
            }),
        )
        .unwrap_err();
        assert_eq!(err.message, LEFT_THERE_DELEGATED);
        let left = err.left.expect("the seat left there");
        assert_eq!(left.remote_id, "0a1b2c3d");
        assert_eq!(
            left.sent.as_deref(),
            Some(token_print("synthetic-refresh").as_str())
        );
        assert_eq!(store.snapshot().unwrap().delegated_out[&id], left);
        // The call back: the copy there is the spent one; the live one here is kept aside.
        let tokens = spent.to_oauth_json(Provider::Claude);
        let give = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            assert!(
                args[2].contains(&format!("'--sender-key' '{KEY}'")),
                "{}",
                args[2]
            );
            Ok(format!(
                r#"{{"ok":true,"data":{{"provider":"claude","credential":{tokens}}}}}"#
            ))
        };
        let taken = recall_take_as(
            &store,
            &id,
            "studio.local",
            &left.remote_id,
            &DEFAULT_APP,
            2,
            &give,
            left.sent.as_deref(),
            Some(KEY),
        )
        .unwrap();
        assert_eq!(
            taken.kept.and_then(|c| c.refresh_token).as_deref(),
            Some("synthetic-refresh-2")
        );
        let snapshot = store.snapshot().unwrap();
        assert!(!snapshot.elsewhere.contains_key(&id));
        assert!(snapshot.delegated_out.is_empty());
    }

    /// SB-403: a seat whose lease ran out there answers no sign-in; the one kept here is asked.
    #[test]
    fn a_call_back_after_the_lease_ran_out_keeps_the_sign_in_kept_here() {
        let (_root, store, id) = store_with(AuthKind::OAuth);
        store
            .mark_elsewhere_at(&id, true, 100, Some("studio.local"))
            .unwrap();
        let run = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            assert!(args[2].contains("'--sender-key' 'K'"), "{}", args[2]);
            Ok(
                r#"{"ok":true,"data":{"provider":"claude","credential":null,"lease_ended":true}}"#
                    .into(),
            )
        };
        let taken = recall_take_as(
            &store,
            &id,
            "studio.local",
            "r1",
            &DEFAULT_APP,
            200,
            &run,
            None,
            Some("K"),
        )
        .unwrap();
        assert!(taken.lease_ended);
        assert_eq!(taken.token.as_deref(), Some("synthetic-refresh"));
        assert!(!store.snapshot().unwrap().elsewhere.contains_key(&id));
        assert_eq!(
            store
                .stored_credential(&id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-refresh")
        );
    }
}
