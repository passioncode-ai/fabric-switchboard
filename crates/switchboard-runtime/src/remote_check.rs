//! SB-327: read another machine's Switchboard from here, and check it against this machine's
//! record of the accounts it moved there (`seats.jsonl`). Before this an agent read the other
//! machine with ssh by hand, and an account used by nobody (claude6@ on the Mac Studio: marked
//! used elsewhere here, turned off there, its sign-in refused) or a mark nobody could place
//! (claude10@: used elsewhere, no location, no move recorded) stayed unseen (operator audit
//! 2026-11-10, `docs/reports/2026-11-10-moves-and-organizations`). Reads only, on both machines.

use crate::account_send::{data, remote, Program, Run, TARGET_INVALID, UNREACHABLE};
use crate::seats::Seat;
use serde_json::{json, Value};
use switchboard_core::Snapshot;

/// Reads the other machine's accounts, groups and organizations (three ssh calls).
struct There {
    accounts: Vec<Value>,
    groups: Vec<Value>,
    organizations: Value,
}

fn read(target: &str, program: &Program<'_>, run: &Run<'_>) -> Result<There, String> {
    if !crate::account_send::target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
    let ask = |args: &[&str]| -> Result<Value, String> {
        run(
            &[target.to_owned(), "--".to_owned(), remote(program, args)],
            None,
        )
        .ok()
        .and_then(|out| data(&out))
        .ok_or_else(|| UNREACHABLE.to_owned())
    };
    let list = |v: Value, key: &str| -> Vec<Value> {
        match v {
            Value::Array(a) => a,
            Value::Object(mut o) => match o.remove(key) {
                Some(Value::Array(a)) => a,
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    };
    Ok(There {
        accounts: list(ask(&["--json", "accounts", "list"])?, "accounts"),
        groups: list(ask(&["--json", "project", "list"])?, "projects"),
        organizations: ask(&["--json", "org", "list"])?,
    })
}

/// The other machine's state in short: its accounts (no credential ever leaves it: `accounts
/// list` answers none), groups with how many accounts and folders each holds, its organizations
/// and whether it is connected to PassionCode.ai.
pub fn status(target: &str, program: &Program<'_>, run: &Run<'_>) -> Result<Value, String> {
    let there = read(target, program, run)?;
    let accounts: Vec<Value> = there
        .accounts
        .iter()
        .map(|a| {
            json!({
                "id": a["id"], "label": a["label"], "provider": a["provider"],
                "pool": a["pool"], "enabled": a["enabled"], "elsewhere": a["elsewhere"],
                "location": a["location"], "assigned_to": a["assigned_to"],
                "used_percent": a["usage"]["used_percent"],
                "usage_status": a["usage_health"]["status"],
            })
        })
        .collect();
    let groups: Vec<Value> = there
        .groups
        .iter()
        .map(|g| {
            json!({
                "pool": g["pool"], "name": g["name"],
                "organization": g["organization_name"],
                "accounts": g["accounts"].as_array().map_or(0, Vec::len),
                "folders": g["folders"].as_array().map_or(0, Vec::len),
            })
        })
        .collect();
    let organizations: Vec<Value> = there.organizations["organizations"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|o| json!({"id": o["id"], "name": o["name"], "passioncode": o["passioncode"]}))
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "target": target,
        "accounts": accounts,
        "groups": groups,
        "organizations": organizations,
        "passioncode": {"connected": there.organizations["passioncode"]["connected"]},
    }))
}

/// One thing the check found. `kind`:
/// - `missing_there`: a move recorded here whose account the other machine no longer lists;
/// - `off_there`: the copy there is turned off or marked used elsewhere, so nobody uses it;
/// - `refused_there`: the provider refused the copy there at its last check (it needs a sign-in);
/// - `not_marked_here`: a move recorded here, but this machine uses the account too;
/// - `held_twice`: an account the other machine uses is used here as well (same provider
///   identity), so each machine's renewal ends the other's sign-in;
/// - `no_record`: an account here marked used elsewhere with no location and no recorded move:
///   where it went is not known.
fn finding(kind: &str, here: Option<&str>, label: &Value, there: Option<&Value>) -> Value {
    json!({"kind": kind, "account": here, "label": label, "remote_account": there.map(|t| &t["id"])})
}

fn same_identity(a: &switchboard_core::Account, there: &Value) -> bool {
    let Some(here) = a
        .external_identity
        .as_ref()
        .and_then(|i| i.account_id.as_deref())
    else {
        return false;
    };
    let provider = match a.provider {
        switchboard_core::Provider::Claude => "claude",
        switchboard_core::Provider::Codex => "codex",
    };
    there["provider"] == provider
        && there["external_identity"]["account_id"].as_str() == Some(here)
        && (a
            .external_identity
            .as_ref()
            .and_then(|i| i.organization_id.as_deref())
            .is_none()
            || there["external_identity"]["organization_id"].as_str()
                == a.external_identity
                    .as_ref()
                    .and_then(|i| i.organization_id.as_deref()))
}

/// Checks the other machine against what this one recorded and holds. `seats` is
/// `seats.jsonl` here, oldest first. Answers `{target, findings, counts}`; an empty `findings`
/// is a clean check.
pub fn check(
    snapshot: &Snapshot,
    seats: &[Seat],
    target: &str,
    program: &Program<'_>,
    run: &Run<'_>,
) -> Result<Value, String> {
    let there = read(target, program, run)?;
    let by_id = |id: &str| there.accounts.iter().find(|a| a["id"] == id);
    let mut findings = Vec::new();
    // The newest grant or call back of each account, for this machine's moves to `target`.
    let mut last: Vec<&Seat> = Vec::new();
    for s in seats
        .iter()
        .rev()
        .filter(|s| matches!(s.event.as_str(), "granted" | "recalled"))
    {
        if !last.iter().any(|x| x.account == s.account) {
            last.push(s);
        }
    }
    last.reverse();
    let granted: Vec<&Seat> = last
        .into_iter()
        .filter(|s| s.event == "granted" && s.machine == target)
        .collect();
    for s in &granted {
        let here = snapshot.accounts.iter().find(|a| a.id == s.account);
        let label = here.map_or(Value::Null, |a| json!(a.label));
        if here.is_some_and(|a| a.enabled && !snapshot.elsewhere.contains_key(&a.id)) {
            findings.push(finding("not_marked_here", Some(&s.account), &label, None));
        }
        let copy = s.remote_account.as_deref().and_then(by_id);
        match copy {
            None => findings.push(finding("missing_there", Some(&s.account), &label, None)),
            Some(c) if c["enabled"] != true || c["elsewhere"] == true => {
                findings.push(finding("off_there", Some(&s.account), &label, Some(c)))
            }
            Some(c) if c["usage_health"]["status"] == "failed" => {
                findings.push(finding("refused_there", Some(&s.account), &label, Some(c)))
            }
            Some(_) => {}
        }
    }
    for c in there
        .accounts
        .iter()
        .filter(|c| c["enabled"] == true && c["elsewhere"] != true)
    {
        if let Some(a) = snapshot
            .accounts
            .iter()
            .find(|a| a.enabled && !snapshot.elsewhere.contains_key(&a.id) && same_identity(a, c))
        {
            findings.push(finding("held_twice", Some(&a.id), &json!(a.label), Some(c)));
        }
    }
    for a in snapshot.accounts.iter().filter(|a| {
        snapshot.elsewhere.contains_key(&a.id)
            && !snapshot.locations.contains_key(&a.id)
            && !seats.iter().any(|s| s.account == a.id)
    }) {
        findings.push(finding("no_record", Some(&a.id), &json!(a.label), None));
    }
    let mut counts = serde_json::Map::new();
    for f in &findings {
        let kind = f["kind"].as_str().unwrap_or_default().to_owned();
        let n = counts.get(&kind).and_then(Value::as_u64).unwrap_or(0);
        counts.insert(kind, json!(n + 1));
    }
    Ok(json!({
        "target": target,
        "moved_there": granted.len(),
        "findings": findings,
        "counts": counts,
    }))
}

/// SB-326: carries this machine's settings to another one: the signed `switchboard-settings/1`
/// file goes on ssh's standard input to `settings import /dev/stdin --signer <this key>` there
/// (a preview, or with `apply` the import), so groups, organizations, agents' settings and
/// rotation settings match without a file copied by hand. The answer is the other machine's
/// import answer. No account, credential or capability is in the file.
pub fn sync(
    bundle: &[u8],
    signer: &str,
    apply: bool,
    target: &str,
    program: &Program<'_>,
    run: &Run<'_>,
) -> Result<Value, String> {
    if !crate::account_send::target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
    let mut args = vec![
        "--json",
        "settings",
        "import",
        "/dev/stdin",
        "--signer",
        signer,
    ];
    if apply {
        args.push("--apply");
    }
    let line = [target.to_owned(), "--".to_owned(), remote(program, &args)];
    match run(&line, Some(bundle)) {
        Ok(out) => data(&out).ok_or_else(|| UNREACHABLE.to_owned()),
        Err(out) => Err(
            serde_json::from_str::<Value>(out.lines().last().unwrap_or(""))
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_owned))
                .unwrap_or_else(|| UNREACHABLE.to_owned()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_send::DEFAULT_APP;
    use std::sync::Arc;
    use switchboard_core::{AuthKind, Credential, MemoryVault, Provider, Store};

    fn account(store: &Store, label: &str, refresh: &str) -> String {
        store
            .add(
                label.into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                Credential {
                    access_token: format!("access-{refresh}"),
                    refresh_token: Some(refresh.into()),
                    id_token: None,
                    native_context: None,
                    expires_at: Some(2_000_000_000),
                    account_id: None,
                },
            )
            .unwrap()
            .id
    }
    fn grant(account: &str, remote: &str) -> Seat {
        let mut s = crate::seats::of(1, "granted", account, "studio.local", "default", None);
        s.remote_account = Some(remote.into());
        s
    }
    fn fake(accounts: Value) -> impl Fn(&[String], Option<&[u8]>) -> Result<String, String> {
        move |args: &[String], _| {
            assert_eq!(args[0], "studio.local");
            let c = &args[2];
            let data = if c.contains("'accounts' 'list'") {
                json!({"accounts": accounts})
            } else if c.contains("'project' 'list'") {
                json!([{"pool": "agents", "name": "Agents", "organization_name": null, "accounts": [], "folders": ["/a"]}])
            } else if c.contains("'org' 'list'") {
                json!({"organizations": [{"id": "o1", "name": "Acme", "passioncode": false}], "passioncode": {"connected": false}})
            } else {
                return Err("unexpected".into());
            };
            Ok(json!({"ok": true, "data": data}).to_string())
        }
    }

    #[test]
    fn a_clean_move_finds_nothing_and_each_broken_one_is_named() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let fine = account(&store, "fine@example.test", "r1");
        let gone = account(&store, "gone@example.test", "r2");
        let off = account(&store, "off@example.test", "r3");
        let refused = account(&store, "refused@example.test", "r4");
        let unmarked = account(&store, "unmarked@example.test", "r5");
        let orphan = account(&store, "orphan@example.test", "r6");
        for id in [&fine, &gone, &off, &refused] {
            store
                .mark_elsewhere_at(id, true, 1, Some("studio.local"))
                .unwrap();
        }
        // A mark with no location and no move: where it went is not known.
        store.mark_elsewhere(&orphan, true, 1).unwrap();
        let seats = vec![
            grant(&fine, "t-fine"),
            grant(&gone, "t-gone"),
            grant(&off, "t-off"),
            grant(&refused, "t-refused"),
            grant(&unmarked, "t-unmarked"),
        ];
        let there = json!([
            {"id": "t-fine", "label": "fine@example.test", "provider": "claude", "enabled": true, "elsewhere": false, "usage_health": {"status": "ok"}},
            {"id": "t-off", "label": "off@example.test", "provider": "claude", "enabled": false, "elsewhere": false},
            {"id": "t-refused", "label": "refused@example.test", "provider": "claude", "enabled": true, "elsewhere": false, "usage_health": {"status": "failed"}},
            {"id": "t-unmarked", "label": "unmarked@example.test", "provider": "claude", "enabled": true, "elsewhere": false, "usage_health": {"status": "ok"}},
        ]);
        let answer = check(
            &store.snapshot().unwrap(),
            &seats,
            "studio.local",
            &DEFAULT_APP,
            &fake(there),
        )
        .unwrap();
        let found: Vec<(String, String)> = answer["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["kind"].as_str().unwrap().to_owned(),
                    f["label"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            found,
            vec![
                ("missing_there".into(), "gone@example.test".into()),
                ("off_there".into(), "off@example.test".into()),
                ("refused_there".into(), "refused@example.test".into()),
                ("not_marked_here".into(), "unmarked@example.test".into()),
                ("no_record".into(), "orphan@example.test".into()),
            ]
        );
        assert_eq!(answer["moved_there"], 5);
        assert_eq!(answer["counts"]["missing_there"], 1);
    }

    #[test]
    fn a_call_back_ends_what_the_check_expects_there() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let back = account(&store, "back@example.test", "r1");
        let mut recalled = crate::seats::of(2, "recalled", &back, "studio.local", "default", None);
        recalled.remote_account = Some("t-back".into());
        let answer = check(
            &store.snapshot().unwrap(),
            &[grant(&back, "t-back"), recalled],
            "studio.local",
            &DEFAULT_APP,
            &fake(json!([])),
        )
        .unwrap();
        assert_eq!(answer["findings"], json!([]));
        assert_eq!(answer["moved_there"], 0);
    }

    #[test]
    fn status_reads_accounts_groups_and_organizations_without_a_credential() {
        let answer = status(
            "studio.local",
            &DEFAULT_APP,
            &fake(json!([{"id": "t1", "label": "a@example.test", "provider": "claude", "pool": "default", "enabled": true, "elsewhere": false, "usage": {"used_percent": 61.0}, "usage_health": {"status": "ok"}}])),
        )
        .unwrap();
        assert_eq!(answer["accounts"][0]["used_percent"], 61.0);
        assert_eq!(answer["groups"][0]["folders"], 1);
        assert_eq!(answer["organizations"][0]["name"], "Acme");
        assert_eq!(answer["passioncode"]["connected"], false);
        assert_eq!(
            status("-oProxyCommand=x", &DEFAULT_APP, &fake(json!([]))).unwrap_err(),
            TARGET_INVALID
        );
    }
    #[test]
    fn settings_go_to_the_other_machine_signed_on_its_standard_input() {
        // SB-326: one ssh call; the file on stdin, the key as the signer, `--apply` only when
        // asked; the other machine's refusal comes back as it said it.
        let seen = std::sync::Mutex::new(Vec::new());
        let program = Program {
            path: "/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard",
            data_dir: None,
        };
        let ok = |args: &[String], stdin: Option<&[u8]>| -> Result<String, String> {
            seen.lock()
                .unwrap()
                .push((args.to_vec(), stdin.map(<[u8]>::to_vec)));
            Ok(r#"{"ok":true,"data":{"preview":{"groups":[]},"applied":false}}"#.into())
        };
        let answer = sync(b"{\"format\":1}", "KEY", false, "me@studio", &program, &ok).unwrap();
        assert_eq!(answer["applied"], false);
        let calls = seen.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        let (args, stdin) = &calls[0];
        assert_eq!(args[0], "me@studio");
        assert!(args[2].ends_with("'settings' 'import' '/dev/stdin' '--signer' 'KEY'"));
        assert_eq!(stdin.as_deref(), Some(&b"{\"format\":1}"[..]));
        sync(b"{}", "KEY", true, "me@studio", &program, &ok).unwrap();
        assert!(seen.lock().unwrap()[1].0[2].ends_with("'--apply'"));
        let refused = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err(r#"{"ok":false,"error":"Settings file signature invalid"}"#.into())
        };
        assert_eq!(
            sync(b"{}", "KEY", true, "me@studio", &program, &refused).unwrap_err(),
            "Settings file signature invalid"
        );
        assert_eq!(
            sync(b"{}", "KEY", true, "-oProxy=x", &program, &ok).unwrap_err(),
            TARGET_INVALID
        );
    }
}
