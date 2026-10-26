//! The OpenRouter key agents run on (SB-79, packet XA-02): its status with what the key may still
//! spend, read from OpenRouter itself. The key's value leaves the vault only for a launched
//! agent's environment (`AgentKeyValue`, the CLI's `agents key --service openrouter`); every other
//! answer carries metadata and numbers only.
use serde_json::{json, Value};
use std::time::Duration;
use switchboard_core::Store;

/// OpenRouter's key endpoint; a test points the base at a loopback stand-in.
const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";
/// A loopback stand-in for OpenRouter in tests. Only `http://127.0.0.1:<port>` is accepted, so the
/// environment can never send the key to another host.
pub const OPENROUTER_BASE_ENV: &str = "SWITCHBOARD_OPENROUTER_BASE";
const TIMEOUT: Duration = Duration::from_secs(10);

pub const KEY_REFUSED: &str =
    "OpenRouter refused this key. Check it on openrouter.ai/settings/keys, then save it again.";
pub const UNREACHABLE: &str =
    "OpenRouter did not answer. The key is saved; its balance shows when OpenRouter answers.";

fn base() -> String {
    pick_base(std::env::var(OPENROUTER_BASE_ENV).ok())
}
fn pick_base(configured: Option<String>) -> String {
    configured
        .filter(|b| b.starts_with("http://127.0.0.1:") && !b.contains('@'))
        .unwrap_or_else(|| OPENROUTER_BASE.into())
}

/// What a key may spend, from `GET /key`: numbers and its label, never the key.
pub async fn credit(key: &str) -> Result<Value, &'static str> {
    credit_at(&base(), key).await
}
async fn credit_at(base: &str, key: &str) -> Result<Value, &'static str> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| UNREACHABLE)?;
    let response = client
        .get(format!("{base}/key"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| UNREACHABLE)?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(KEY_REFUSED),
        _ => return Err(UNREACHABLE),
    }
    let body: Value = response.json().await.map_err(|_| UNREACHABLE)?;
    let data = body.get("data").ok_or(UNREACHABLE)?;
    let number = |name: &str| data.get(name).and_then(Value::as_f64);
    let label = data.get("label").and_then(Value::as_str).map(|l| {
        l.chars()
            .filter(|c| !c.is_control())
            .take(80)
            .collect::<String>()
    });
    Ok(json!({
        "label": label,
        "limit": number("limit"),
        "limit_remaining": number("limit_remaining"),
        "limit_reset": data.get("limit_reset").and_then(Value::as_str),
        "usage": number("usage"),
        "usage_daily": number("usage_daily"),
        "is_free_tier": data.get("is_free_tier").and_then(Value::as_bool),
    }))
}

/// One key's spend for the hand-over gate (C-5): the numbers of a [`credit`] answer.
pub(crate) fn key_read(credit: &Value) -> crate::spend::KeyRead {
    let number = |name: &str| credit.get(name).and_then(Value::as_f64);
    crate::spend::KeyRead {
        limit_remaining: number("limit_remaining"),
        usage: number("usage"),
        usage_daily: number("usage_daily"),
    }
}

/// The spend of the saved OpenRouter key `pool` names (none: the machine's), asked of OpenRouter
/// at a paid hand-over (C-5) and nowhere else. `None` when no such key is saved, its value cannot
/// be read from the vault, or OpenRouter refused or did not answer: an unknown, never a zero.
pub(crate) async fn spend_read(store: &Store, pool: Option<&str>) -> Option<crate::spend::KeyRead> {
    spend_read_at(store, pool, &base()).await
}
async fn spend_read_at(
    store: &Store,
    pool: Option<&str>,
    base: &str,
) -> Option<crate::spend::KeyRead> {
    let entry = store.agent_key_for("openrouter", pool).ok()??;
    let secret = store.agent_key_secret(&entry).ok()?;
    let credit = credit_at(base, &secret).await.ok()?;
    Some(key_read(&credit))
}

/// The spend of the key an agent started in `folder` runs on (`Store::agent_key_in`, the rule
/// [`crate::spend::key_slot`] shares), or `None` as [`spend_read`].
pub async fn credit_for(
    store: &Store,
    folder: Option<&std::path::Path>,
) -> Option<crate::spend::KeyRead> {
    let entry = store.agent_key_in("openrouter", folder).ok()??;
    spend_read(store, entry.pool.as_deref()).await
}

/// Every saved OpenRouter key but the one `slot` names, by its pool (none: the machine's): the
/// keys a machine ceiling adds up besides the step's own.
pub(crate) fn other_keys(
    snapshot: &switchboard_core::Snapshot,
    slot: &crate::spend::KeySlot,
) -> Vec<Option<String>> {
    snapshot
        .agent_keys
        .iter()
        .filter(|k| k.service == "openrouter")
        .filter(|k| match (slot, &k.pool) {
            (crate::spend::KeySlot::Machine, None) => false,
            (crate::spend::KeySlot::Group(p), Some(q)) => p != q,
            _ => true,
        })
        .map(|k| k.pool.clone())
        .collect()
}

/// `{service, saved, saved_at, model, credit, credit_error}` — `credit` asked of OpenRouter when
/// `with_credit` and a key is saved.
pub async fn status(store: &Store, service: &str, with_credit: bool) -> Result<Value, String> {
    status_at(store, service, None, with_credit, &base()).await
}
/// As `status`, for one project pool's key (SB-144); none: the machine's key.
pub async fn status_for(
    store: &Store,
    service: &str,
    pool: Option<&str>,
    with_credit: bool,
) -> Result<Value, String> {
    status_at(store, service, pool, with_credit, &base()).await
}
/// SB-144: `{service, keys: [{pool, project, saved_at, model}], projects: [{pool, name, own}]}`:
/// the machine's key (`pool: null`) and each project's own, and every project with whether it has
/// one. Never a key.
pub fn list(store: &Store, service: &str) -> Result<Value, String> {
    let snapshot = store.snapshot()?;
    let name = |pool: &Option<String>| {
        pool.as_ref().and_then(|p| {
            snapshot
                .projects
                .iter()
                .find(|x| &x.pool == p)
                .map(|x| x.name.clone())
        })
    };
    let keys: Vec<Value> = snapshot
        .agent_keys
        .iter()
        .filter(|k| k.service == service)
        .map(|k| json!({"pool": k.pool, "project": name(&k.pool), "saved_at": k.saved_at, "model": k.model}))
        .collect();
    let projects: Vec<Value> = snapshot
        .projects
        .iter()
        .map(|p| {
            let own = snapshot
                .agent_keys
                .iter()
                .any(|k| k.service == service && k.pool.as_ref() == Some(&p.pool));
            json!({"pool": p.pool, "name": p.name, "own": own})
        })
        .collect();
    Ok(json!({"service": service, "keys": keys, "projects": projects}))
}
async fn status_at(
    store: &Store,
    service: &str,
    pool: Option<&str>,
    with_credit: bool,
    base: &str,
) -> Result<Value, String> {
    let entry = store.agent_key_for(service, pool)?;
    let Some(entry) = entry else {
        return Ok(
            json!({"service": service, "pool": pool, "saved": false, "saved_at": null, "model": null, "credit": null, "credit_error": null}),
        );
    };
    let (credit, credit_error) = if with_credit {
        match credit_at(base, &store.agent_key_secret(&entry)?).await {
            Ok(credit) => (credit, Value::Null),
            Err(error) => (Value::Null, json!(error)),
        }
    } else {
        (Value::Null, Value::Null)
    };
    Ok(json!({
        "service": service,
        "pool": pool,
        "saved": true,
        "saved_at": entry.saved_at,
        "model": entry.model,
        "credit": credit,
        "credit_error": credit_error,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderMap, routing::get, Json, Router};
    use std::sync::Arc;
    use switchboard_core::MemoryVault;

    async fn stand_in(status: u16) -> String {
        let app = Router::new().route(
            "/key",
            get(move |headers: HeaderMap| async move {
                let auth = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_owned();
                let code = if auth == "Bearer sk-or-v1-synthetic" {
                    status
                } else {
                    401
                };
                (
                    axum::http::StatusCode::from_u16(code).unwrap(),
                    Json(json!({"data": {"label": "agents\u{7}", "limit": 5.0, "limit_remaining": 3.25, "limit_reset": "daily", "usage": 11.5, "usage_daily": 1.75, "is_free_tier": false}})),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn the_status_reports_what_the_key_may_spend_and_never_the_key() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
        let base = stand_in(200).await;
        let empty = status_at(&store, "openrouter", None, true, &base)
            .await
            .unwrap();
        assert_eq!(empty["saved"], false);
        store
            .set_agent_key(
                "openrouter",
                "sk-or-v1-synthetic",
                Some("moonshotai/kimi-k2".into()),
            )
            .unwrap();
        let answer = status_at(&store, "openrouter", None, true, &base)
            .await
            .unwrap();
        assert_eq!(answer["saved"], true);
        assert_eq!(answer["model"], "moonshotai/kimi-k2");
        assert_eq!(answer["credit"]["limit_remaining"], 3.25);
        assert_eq!(answer["credit"]["usage_daily"], 1.75);
        assert_eq!(
            answer["credit"]["label"], "agents",
            "control characters dropped"
        );
        assert!(
            !answer.to_string().contains("sk-or-v1"),
            "never the key: {answer}"
        );
        // A refused key is named as such; the key stays saved.
        store
            .set_agent_key("openrouter", "sk-or-v1-revoked", None)
            .unwrap();
        let refused = status_at(&store, "openrouter", None, true, &base)
            .await
            .unwrap();
        assert_eq!(refused["credit_error"], KEY_REFUSED);
        assert!(refused["credit"].is_null());
        // A host other than loopback is never taken from the environment.
        assert_eq!(
            pick_base(Some("https://attacker.example".into())),
            OPENROUTER_BASE
        );
        assert_eq!(
            pick_base(Some("http://127.0.0.1:1@evil".into())),
            OPENROUTER_BASE
        );
        assert_eq!(
            pick_base(Some("http://127.0.0.1:9".into())),
            "http://127.0.0.1:9"
        );
        assert_eq!(pick_base(None), OPENROUTER_BASE);
    }

    #[tokio::test]
    async fn the_gate_reads_a_saved_keys_spend_and_an_unanswered_read_is_unknown() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
        let base = stand_in(200).await;
        assert_eq!(
            spend_read_at(&store, None, &base).await,
            None,
            "no key saved"
        );
        store
            .set_agent_key("openrouter", "sk-or-v1-synthetic", None)
            .unwrap();
        let read = spend_read_at(&store, None, &base).await.unwrap();
        assert_eq!(read.limit_remaining, Some(3.25));
        assert_eq!(read.usage, Some(11.5));
        assert_eq!(read.usage_daily, Some(1.75));
        assert_eq!(
            spend_read_at(&store, Some("alpha"), &base).await,
            None,
            "a group without its own key"
        );
        let down = stand_in(500).await;
        assert_eq!(
            spend_read_at(&store, None, &down).await,
            None,
            "unknown, never zero"
        );
        store
            .set_agent_key("openrouter", "sk-or-v1-revoked", None)
            .unwrap();
        assert_eq!(
            spend_read_at(&store, None, &base).await,
            None,
            "a refused key"
        );
        // The other keys a machine ceiling adds up: every saved one but the step's own.
        let snapshot = switchboard_core::Snapshot {
            agent_keys: ["machine", "alpha", "beta"]
                .iter()
                .map(|p| switchboard_core::agent_keys::AgentKey {
                    id: format!("k-{p}"),
                    service: "openrouter".into(),
                    pool: (*p != "machine").then(|| (*p).to_string()),
                    saved_at: 1,
                    model: None,
                })
                .collect(),
            ..Default::default()
        };
        use crate::spend::KeySlot;
        assert_eq!(
            other_keys(&snapshot, &KeySlot::Machine),
            [Some("alpha".to_string()), Some("beta".to_string())]
        );
        assert_eq!(
            other_keys(&snapshot, &KeySlot::Group("alpha".into())),
            [None, Some("beta".to_string())]
        );
    }
}
