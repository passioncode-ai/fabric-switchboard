//! Project rules at the runtime boundary: canonical folders, and applying a rule to the
//! session that asked. A rule never replaces rotation; it names a starting account.
use crate::{activate_native, monitor, NativeSources, Runtime, NATIVE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use switchboard_core::{Account, ProjectRule, Provider, RuleResolution, Store};

/// Where the calling agent's requests go. Managed sessions carry their pool; a native
/// session signs in through the ordinary provider CLI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Session {
    Managed {
        provider: Provider,
        pool: String,
    },
    Isolated {
        provider: Provider,
        account_id: String,
    },
    Native {
        provider: Option<Provider>,
    },
}
impl Session {
    /// `SWITCHBOARD_SESSION`, written by a Switchboard launch: `managed:<provider>:<pool>`
    /// or `isolated:<provider>:<account id>`.
    pub fn from_label(label: &str) -> Option<Self> {
        let mut parts = label.splitn(3, ':');
        let (mode, provider, rest) = (parts.next()?, parts.next()?, parts.next()?);
        let provider = match provider {
            "claude" => Provider::Claude,
            "codex" => Provider::Codex,
            _ => return None,
        };
        let bounded = !rest.is_empty()
            && rest.len() <= 64
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        match mode {
            "managed" if bounded => Some(Self::Managed {
                provider,
                pool: rest.into(),
            }),
            "isolated" if bounded => Some(Self::Isolated {
                provider,
                account_id: rest.into(),
            }),
            _ => None,
        }
    }
    pub fn label(&self) -> Option<String> {
        match self {
            Self::Managed { provider, pool } => {
                Some(format!("managed:{}:{pool}", provider.as_str()))
            }
            Self::Isolated {
                provider,
                account_id,
            } => Some(format!("isolated:{}:{account_id}", provider.as_str())),
            Self::Native { .. } => None,
        }
    }
}

/// Which session is asking, from the environment of the asking process. A base URL only
/// counts when it names the running proxy, so a stale variable cannot claim a pool.
pub fn detect_session(
    var: impl Fn(&str) -> Option<String>,
    proxy_address: Option<&str>,
    root: &Path,
) -> Session {
    if let Some(session) = var("SWITCHBOARD_SESSION").and_then(|v| Session::from_label(&v)) {
        return session;
    }
    if let (Some(url), Some(proxy)) = (var("ANTHROPIC_BASE_URL"), proxy_address) {
        let prefix = format!("http://{proxy}/claude/");
        if let Some(pool) = url.strip_prefix(&prefix).map(|p| p.trim_end_matches('/')) {
            if let Some(Session::Managed { provider, pool }) =
                Session::from_label(&format!("managed:claude:{pool}"))
            {
                return Session::Managed { provider, pool };
            }
        }
    }
    if let Some(home) = var("CODEX_HOME") {
        let runtimes = root.join("runtimes");
        if let Some(pool) = Path::new(&home)
            .strip_prefix(&runtimes)
            .ok()
            .and_then(|p| p.to_str())
            .and_then(|p| p.strip_prefix("codex-"))
        {
            if let Some(session) = Session::from_label(&format!("managed:codex:{pool}")) {
                return session;
            }
        }
    }
    let claude = var("CLAUDECODE").is_some_and(|v| v == "1");
    Session::Native {
        provider: claude.then_some(Provider::Claude),
    }
}

pub(crate) fn project_dir(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute project folder.".into());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "Choose an existing project folder.")?;
    let private = root.canonicalize().unwrap_or_else(|_| root.to_owned());
    if !path.is_dir() || path.starts_with(&private) {
        return Err("Choose an existing project folder.".into());
    }
    Ok(path)
}
/// A removed project folder can still have its rule deleted.
pub(crate) fn rule_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    project_dir(root, path).or_else(|error| {
        if path.is_absolute() && !path.exists() {
            Ok(path.to_owned())
        } else {
            Err(error)
        }
    })
}

fn account_view(account: &Account) -> Value {
    json!({"id": account.id, "label": account.label, "provider": account.provider, "pool": account.pool, "enabled": account.enabled})
}
fn rule_state(rule: &ProjectRule, now: i64) -> &'static str {
    if !rule.enabled {
        "paused"
    } else if !rule.in_force(now) {
        "expired"
    } else {
        "active"
    }
}
pub(crate) fn rule_view(store: &Store, rule: &ProjectRule, now: i64) -> Value {
    let account = store
        .snapshot()
        .ok()
        .and_then(|s| s.accounts.into_iter().find(|a| a.id == rule.account_id));
    json!({
        "path": rule.path, "provider": rule.provider, "target": rule.target,
        "enabled": rule.enabled, "state": rule_state(rule, now),
        "created_at": rule.created_at, "expires_at": rule.expires_at,
        "account": account.as_ref().map(account_view),
    })
}

pub(crate) fn resolve(store: &Store, path: &Path, now: i64) -> Result<Value, String> {
    let snapshot = store.snapshot()?;
    let results: Vec<Value> = store
        .resolve_rules(path, now)?
        .into_iter()
        .map(
            |RuleResolution {
                 provider,
                 effective,
                 nearest,
             }| {
                let in_effect = effective.as_ref().map(|rule| {
                    rule.target == "managed"
                        && snapshot
                            .accounts
                            .iter()
                            .find(|a| a.id == rule.account_id)
                            .is_some_and(|a| {
                                snapshot
                                    .routes
                                    .get(&format!("{}:{}", a.provider.as_str(), a.pool))
                                    == Some(&a.id)
                            })
                });
                json!({
                    "provider": provider,
                    "effective": effective.as_ref().map(|r| rule_view(store, r, now)),
                    "nearest": nearest.as_ref().map(|r| rule_view(store, r, now)),
                    "managed_route_in_effect": in_effect,
                })
            },
        )
        .collect();
    Ok(json!({"path": path, "rules": results}))
}

fn outcome(provider: Provider, action: &str, account: Option<&Account>, message: String) -> Value {
    json!({"provider": provider, "action": action, "account": account.map(account_view), "message": message})
}

/// Applies the effective rule for each provider this session can use. Nothing changes
/// when the rule would reach another session, another pool, or the global login
/// without `global`.
pub(crate) fn apply(
    store: &Store,
    runtime: Option<&Runtime>,
    path: &Path,
    session: &Session,
    global: bool,
) -> Result<Value, String> {
    let now = monitor::now();
    let native = runtime.map_or(NATIVE, |r| r.native);
    let snapshot = store.snapshot()?;
    let mut results = Vec::new();
    for RuleResolution {
        provider,
        effective,
        nearest,
    } in store.resolve_rules(path, now)?
    {
        let relevant = match session {
            Session::Managed { provider: p, .. } | Session::Isolated { provider: p, .. } => {
                *p == provider
            }
            Session::Native { provider: p } => p.is_none_or(|p| p == provider),
        };
        if !relevant {
            continue;
        }
        let Some(rule) = effective else {
            let (action, message) = match nearest {
                Some(rule) if !rule.enabled => ("rule_paused", "The project rule is paused. Switchboard keeps its current selection and rotation.".to_string()),
                Some(_) => ("rule_expired", "The project rule has expired. Switchboard keeps its current selection and rotation.".to_string()),
                None => ("no_rule", "No project rule for this folder. Switchboard keeps its current selection and rotation.".to_string()),
            };
            results.push(outcome(provider, action, None, message));
            continue;
        };
        let Some(account) = snapshot.accounts.iter().find(|a| a.id == rule.account_id) else {
            results.push(outcome(
                provider,
                "failed",
                None,
                "The rule's account no longer exists. Remove or edit the rule.".into(),
            ));
            continue;
        };
        let result = match (rule.target.as_str(), session) {
            (_, Session::Isolated { .. }) => outcome(
                provider,
                "other_session",
                Some(account),
                "This session is isolated to one account; project rules do not change it. Launch a managed session to follow rules.".into(),
            ),
            ("managed", Session::Managed { pool, .. }) if *pool != account.pool => outcome(
                provider,
                "other_pool",
                Some(account),
                format!("This session routes the {pool} pool; the rule's account is in {}. Launch a managed session in {} to use it.", account.pool, account.pool),
            ),
            ("managed", Session::Managed { .. }) => {
                if snapshot.routes.get(&format!("{}:{}", provider.as_str(), account.pool)) == Some(&account.id) {
                    outcome(provider, "already_in_effect", Some(account), format!("{} already handles the next request in the {} pool.", account.label, account.pool))
                } else {
                    match store.select_with_cooldown(provider, &account.pool, &account.id, now) {
                        Ok(()) => {
                            store.record_rule_applied(&account.id)?;
                            outcome(provider, "selected", Some(account), format!("{} handles the next request in the {} pool. A response already streaming keeps its account.", account.label, account.pool))
                        }
                        Err(error) => outcome(provider, "failed", Some(account), error),
                    }
                }
            }
            ("managed", Session::Native { .. }) => outcome(
                provider,
                "other_session",
                Some(account),
                "This rule routes managed sessions, and this session signs in natively. Launch a managed session to use it.".into(),
            ),
            (_, Session::Managed { .. }) => outcome(
                provider,
                "other_session",
                Some(account),
                "This rule changes the ordinary Claude Code login, and this session runs through Switchboard's proxy. Nothing was changed.".into(),
            ),
            (_, Session::Native { .. }) => {
                if native_matches(store, account, native) {
                    outcome(provider, "already_in_effect", Some(account), format!("Claude Code already signs in as {}.", account.label))
                } else if !global {
                    outcome(provider, "needs_global", Some(account), "This rule changes the Claude Code login for every ordinary claude session on this machine. Call again with global: true to apply it.".into())
                } else {
                    match activate_native(store, &account.id, None, native, runtime.map(|r| &r.refresh)) {
                        Ok(()) => {
                            if let Some(runtime) = runtime {
                                runtime.invalidate_current();
                                runtime.limits.switched(
                                    account.external_identity.as_ref(),
                                    crate::monitor::now(),
                                );
                            }
                            store.record_rule_applied(&account.id)?;
                            outcome(provider, "activated", Some(account), format!("Claude Code now signs in as {} for every ordinary claude session. Running sessions may keep their account until they reload credentials.", account.label))
                        }
                        Err(error) => outcome(provider, "failed", Some(account), error),
                    }
                }
            }
        };
        results.push(result);
    }
    Ok(json!({"path": path, "session": session, "results": results}))
}

fn native_matches(store: &Store, account: &Account, native: NativeSources) -> bool {
    (native.current)(Provider::Claude)
        .ok()
        .and_then(|current| {
            store
                .match_external(Provider::Claude, &account.pool, &current.identity)
                .ok()
                .flatten()
        })
        .is_some_and(|matched| matched.id == account.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use switchboard_core::{AuthKind, Credential, MemoryVault};

    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Store) {
        let data = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        let store = Store::open(data.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        (data, projects, store)
    }
    fn add(store: &Store, pool: &str, secret: &str) -> Account {
        store
            .add(
                format!("Synthetic {secret}"),
                Provider::Claude,
                AuthKind::ApiKey,
                pool.into(),
                Credential::parse(Provider::Claude, AuthKind::ApiKey, secret).unwrap(),
            )
            .unwrap()
    }
    fn actions(value: &Value) -> Vec<String> {
        value["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["action"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn folders_are_canonical_existing_and_outside_app_data() {
        let (data, projects, _store) = fixture();
        assert!(project_dir(data.path(), Path::new("relative")).is_err());
        assert!(project_dir(data.path(), data.path()).is_err());
        assert!(project_dir(data.path(), &projects.path().join("missing")).is_err());
        let nested = projects.path().join("alpha");
        std::fs::create_dir(&nested).unwrap();
        let dotted = projects.path().join("alpha/../alpha");
        assert_eq!(
            project_dir(data.path(), &dotted).unwrap(),
            nested.canonicalize().unwrap()
        );
        assert_eq!(
            rule_path(data.path(), &projects.path().join("gone")).unwrap(),
            projects.path().join("gone")
        );
    }

    #[test]
    fn managed_rule_selects_once_in_its_pool_and_never_crosses_pools() {
        let (data, projects, store) = fixture();
        let first = add(&store, "work", "synthetic-first");
        let second = add(&store, "work", "synthetic-second");
        let personal = add(&store, "personal", "synthetic-personal");
        store.select(Provider::Claude, "work", &first.id).unwrap();
        let alpha = project_dir(data.path(), projects.path()).unwrap();
        store
            .set_rule(&alpha, &second.id, "managed", true, None)
            .unwrap();
        let session = Session::Managed {
            provider: Provider::Claude,
            pool: "work".into(),
        };
        let applied = apply(&store, None, &alpha, &session, false).unwrap();
        assert_eq!(actions(&applied), ["selected"]);
        assert_eq!(store.snapshot().unwrap().routes["claude:work"], second.id);
        let again = apply(&store, None, &alpha, &session, false).unwrap();
        assert_eq!(actions(&again), ["already_in_effect"]);

        store
            .set_rule(&alpha, &personal.id, "managed", true, None)
            .unwrap();
        let crossed = apply(&store, None, &alpha, &session, false).unwrap();
        assert_eq!(actions(&crossed), ["other_pool"]);
        assert_eq!(store.snapshot().unwrap().routes["claude:work"], second.id);
        let native = apply(
            &store,
            None,
            &alpha,
            &Session::Native {
                provider: Some(Provider::Claude),
            },
            true,
        )
        .unwrap();
        assert_eq!(actions(&native), ["other_session"]);
        let isolated = Session::Isolated {
            provider: Provider::Claude,
            account_id: first.id.clone(),
        };
        assert_eq!(
            actions(&apply(&store, None, &alpha, &isolated, true).unwrap()),
            ["other_session"]
        );
    }

    #[test]
    fn sessions_are_detected_from_launch_labels_the_live_proxy_and_codex_homes() {
        let root = Path::new("/data/switchboard");
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            detect_session(
                env(&[
                    ("SWITCHBOARD_SESSION", "managed:codex:work"),
                    ("CLAUDECODE", "1")
                ]),
                None,
                root
            ),
            Session::Managed {
                provider: Provider::Codex,
                pool: "work".into()
            }
        );
        let url = &[(
            "ANTHROPIC_BASE_URL",
            "http://127.0.0.1:4100/claude/personal",
        )];
        assert_eq!(
            detect_session(env(url), Some("127.0.0.1:4100"), root),
            Session::Managed {
                provider: Provider::Claude,
                pool: "personal".into()
            }
        );
        assert_eq!(
            detect_session(env(url), Some("127.0.0.1:4200"), root),
            Session::Native { provider: None }
        );
        assert_eq!(
            detect_session(env(url), None, root),
            Session::Native { provider: None }
        );
        assert_eq!(
            detect_session(
                env(&[("CODEX_HOME", "/data/switchboard/runtimes/codex-work")]),
                None,
                root
            ),
            Session::Managed {
                provider: Provider::Codex,
                pool: "work".into()
            }
        );
        assert_eq!(
            detect_session(
                env(&[("CODEX_HOME", "/elsewhere/runtimes/codex-work")]),
                None,
                root
            ),
            Session::Native { provider: None }
        );
        assert_eq!(
            detect_session(
                env(&[("CLAUDECODE", "1"), ("SWITCHBOARD_SESSION", "garbage")]),
                None,
                root
            ),
            Session::Native {
                provider: Some(Provider::Claude)
            }
        );
    }

    #[test]
    fn session_labels_round_trip_and_reject_foreign_shapes() {
        for session in [
            Session::Managed {
                provider: Provider::Codex,
                pool: "work".into(),
            },
            Session::Isolated {
                provider: Provider::Claude,
                account_id: "00000000-0000-4000-8000-000000000000".into(),
            },
        ] {
            assert_eq!(
                Session::from_label(&session.label().unwrap()),
                Some(session)
            );
        }
        for label in [
            "",
            "managed",
            "managed:claude",
            "managed:claude:",
            "native:claude:x",
            "managed:gemini:work",
            "managed:claude:Work",
            "managed:claude:a/b",
        ] {
            assert_eq!(Session::from_label(label), None, "{label}");
        }
    }

    #[test]
    fn paused_expired_and_missing_rules_change_nothing() {
        let (data, projects, store) = fixture();
        let first = add(&store, "work", "synthetic-first");
        let second = add(&store, "work", "synthetic-second");
        store.select(Provider::Claude, "work", &first.id).unwrap();
        let alpha = project_dir(data.path(), projects.path()).unwrap();
        let session = Session::Managed {
            provider: Provider::Claude,
            pool: "work".into(),
        };
        assert_eq!(
            actions(&apply(&store, None, &alpha, &session, false).unwrap()),
            ["no_rule"]
        );
        store
            .set_rule(&alpha, &second.id, "managed", false, None)
            .unwrap();
        assert_eq!(
            actions(&apply(&store, None, &alpha, &session, false).unwrap()),
            ["rule_paused"]
        );
        assert_eq!(store.snapshot().unwrap().routes["claude:work"], first.id);
        let resolved = resolve(&store, &alpha, monitor::now()).unwrap();
        assert_eq!(resolved["rules"][0]["nearest"]["state"], "paused");
        assert!(resolved["rules"][0]["effective"].is_null());
    }
}
