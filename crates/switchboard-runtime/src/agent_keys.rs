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

/// `{service, saved, saved_at, model, credit, credit_error}` — `credit` asked of OpenRouter when
/// `with_credit` and a key is saved.
pub async fn status(store: &Store, service: &str, with_credit: bool) -> Result<Value, String> {
    status_at(store, service, with_credit, &base()).await
}
async fn status_at(
    store: &Store,
    service: &str,
    with_credit: bool,
    base: &str,
) -> Result<Value, String> {
    let entry = store.agent_key(service)?;
    let Some(entry) = entry else {
        return Ok(
            json!({"service": service, "saved": false, "saved_at": null, "model": null, "credit": null, "credit_error": null}),
        );
    };
    let (credit, credit_error) = if with_credit {
        match credit_at(base, &store.agent_key_value(service)?).await {
            Ok(credit) => (credit, Value::Null),
            Err(error) => (Value::Null, json!(error)),
        }
    } else {
        (Value::Null, Value::Null)
    };
    Ok(json!({
        "service": service,
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
        let empty = status_at(&store, "openrouter", true, &base).await.unwrap();
        assert_eq!(empty["saved"], false);
        store
            .set_agent_key(
                "openrouter",
                "sk-or-v1-synthetic",
                Some("moonshotai/kimi-k2".into()),
            )
            .unwrap();
        let answer = status_at(&store, "openrouter", true, &base).await.unwrap();
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
        let refused = status_at(&store, "openrouter", true, &base).await.unwrap();
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
}
