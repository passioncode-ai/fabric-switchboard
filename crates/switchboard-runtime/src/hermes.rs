//! Hermes Agent's model and provider (SB-80, packet XA-02 A-4). Read from `hermes config get model
//! --json` (Hermes masks secret-shaped values; only four non-secret keys are kept) and changed only
//! on the person's request through Hermes's own `hermes config set` — the one write Switchboard
//! makes to another agent's config. Hermes accepts any string there, so Switchboard checks the
//! shape first; Hermes itself clears a `base_url` that belonged to the previous provider.
use crate::launch::find_program;
use serde_json::{json, Value};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_OUTPUT: usize = 64 * 1024;

pub const HERMES_NOT_INSTALLED: &str =
    "Hermes is not installed. Install it from hermes-agent.nousresearch.com, then try again.";
pub const HERMES_FAILED: &str = "Hermes did not answer. Run hermes config check, then try again.";
pub const PROVIDER_INVALID: &str =
    "A Hermes provider is an id such as openrouter, anthropic or nous (lowercase letters, digits and - . _ :).";
pub const MODEL_INVALID: &str =
    "A model is an id such as moonshotai/kimi-k2 or claude-sonnet-4.5 (letters, digits and / . _ : -).";
pub const NOTHING_TO_SET: &str = "Name a provider, a model or both.";

/// Providers the app offers in its list; any other id of the same shape is accepted too, since
/// Hermes adds providers between releases (82 in 0.21.4).
pub const COMMON_PROVIDERS: &[&str] = &[
    "openrouter",
    "anthropic",
    "nous",
    "openai-codex",
    "kimi-coding",
    "deepseek",
    "gemini",
    "xai",
    "auto",
];

pub fn provider_valid(provider: &str) -> bool {
    let bytes = provider.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 40
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_' | b':')
        })
}
pub fn model_valid(model: &str) -> bool {
    let bytes = model.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 120
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b':' | b'-'))
}

/// Runs Hermes with fixed arguments and a deadline; stdout only, bounded. Never through a shell.
fn run(program: &Path, args: &[&str]) -> Result<String, &'static str> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| HERMES_FAILED)?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = Vec::new();
                if let Some(stdout) = child.stdout.take() {
                    let _ = stdout.take(MAX_OUTPUT as u64 + 1).read_to_end(&mut out);
                }
                if !status.success() || out.len() > MAX_OUTPUT {
                    return Err(HERMES_FAILED);
                }
                return String::from_utf8(out).map_err(|_| HERMES_FAILED);
            }
            Ok(None) if started.elapsed() < TIMEOUT => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HERMES_FAILED);
            }
        }
    }
}

/// An endpoint as shown: scheme, host and path — never a query string or credentials.
fn endpoint(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?;
    let port = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
    Some(format!(
        "{}://{host}{port}{}",
        parsed.scheme(),
        parsed.path().trim_end_matches('/')
    ))
}

/// `{model, provider, base_url, api_mode}` from Hermes's resolved `model` section; anything else
/// in it (keys, tokens) is dropped.
pub fn parse_model(text: &str) -> Value {
    let value: Value = serde_json::from_str(text.trim()).unwrap_or(Value::Null);
    let Some(section) = value.as_object() else {
        // A home without a model section prints an empty string.
        return json!({"configured": false, "model": null, "provider": null, "base_url": null, "api_mode": null});
    };
    let text = |key: &str, valid: fn(&str) -> bool| {
        section
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| valid(v))
            .map(str::to_owned)
    };
    let model = text("default", model_valid).or_else(|| text("model", model_valid));
    let provider = text("provider", provider_valid);
    json!({
        "configured": model.is_some() || provider.is_some(),
        "model": model,
        "provider": provider,
        "base_url": section.get("base_url").and_then(Value::as_str).and_then(endpoint),
        "api_mode": text("api_mode", provider_valid),
    })
}

fn status_with(program: Option<&Path>) -> Value {
    let Some(program) = program else {
        return json!({"installed": false, "configured": false, "error": null, "providers": COMMON_PROVIDERS});
    };
    let mut status = match run(program, &["config", "get", "model", "--json"]) {
        Ok(text) => parse_model(&text),
        Err(error) => json!({"configured": false, "error": error}),
    };
    status["installed"] = json!(true);
    status["providers"] = json!(COMMON_PROVIDERS);
    if status.get("error").is_none() {
        status["error"] = Value::Null;
    }
    status
}

/// The ordinary Hermes's model and provider (its own `HERMES_HOME` or `~/.hermes`).
pub fn status() -> Value {
    status_with(find_program("hermes").as_deref())
}

/// Changes the provider and/or the default model with `hermes config set`, then reads them back.
pub fn set(provider: Option<&str>, model: Option<&str>) -> Result<Value, String> {
    let program = find_program("hermes").ok_or(HERMES_NOT_INSTALLED)?;
    set_with(&program, provider, model)
}
fn set_with(program: &Path, provider: Option<&str>, model: Option<&str>) -> Result<Value, String> {
    let provider = provider.map(str::trim).filter(|p| !p.is_empty());
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    if provider.is_none() && model.is_none() {
        return Err(NOTHING_TO_SET.into());
    }
    if provider.is_some_and(|p| !provider_valid(p)) {
        return Err(PROVIDER_INVALID.into());
    }
    if model.is_some_and(|m| !model_valid(m)) {
        return Err(MODEL_INVALID.into());
    }
    // The provider first: Hermes clears an endpoint that belonged to the previous one.
    if let Some(provider) = provider {
        run(program, &["config", "set", "model.provider", provider])?;
    }
    if let Some(model) = model {
        run(program, &["config", "set", "model.default", model])?;
    }
    Ok(status_with(Some(program)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_four_non_secret_keys_survive_the_read() {
        let status = parse_model(
            r#"{"default": "deepseek/deepseek-v4-pro-0813", "provider": "openrouter",
                "base_url": "https://user:pw@openrouter.ai/api/v1/?token=x", "api_mode": "chat_completions",
                "api_key": "sk-or-***", "key_env": "OPENROUTER_API_KEY"}"#,
        );
        assert_eq!(status["model"], "deepseek/deepseek-v4-pro-0813");
        assert_eq!(status["provider"], "openrouter");
        assert_eq!(status["base_url"], "https://openrouter.ai/api/v1");
        assert_eq!(status["api_mode"], "chat_completions");
        assert_eq!(status["configured"], true);
        let text = status.to_string();
        assert!(!text.contains("sk-or") && !text.contains("pw") && !text.contains("token"));
        // The `model` alias, an empty home and garbage.
        assert_eq!(
            parse_model(r#"{"model": "anthropic/claude-opus-4.6"}"#)["model"],
            "anthropic/claude-opus-4.6"
        );
        assert_eq!(parse_model("\"\"\n")["configured"], false);
        assert_eq!(parse_model("not json")["configured"], false);
        assert_eq!(
            parse_model(r#"{"provider": "bad provider; rm"}"#)["provider"],
            Value::Null
        );
    }

    #[test]
    fn provider_and_model_shapes_are_checked_before_hermes_runs() {
        for ok in [
            "openrouter",
            "kimi-coding",
            "custom:local",
            "router.com",
            "zai",
        ] {
            assert!(provider_valid(ok), "{ok}");
        }
        for bad in ["", "OpenRouter", "-x", "a b", "x;rm", &"a".repeat(41)] {
            assert!(!provider_valid(bad), "{bad}");
        }
        for ok in [
            "moonshotai/kimi-k2",
            "claude-sonnet-4.5",
            "openrouter/auto",
            "x:free",
        ] {
            assert!(model_valid(ok), "{ok}");
        }
        for bad in ["", "bad model;rm", "/leading", "$(id)"] {
            assert!(!model_valid(bad), "{bad}");
        }
        let never = Path::new("/nonexistent/hermes-must-not-run");
        assert_eq!(
            set_with(never, None, Some(" ")).unwrap_err(),
            NOTHING_TO_SET
        );
        assert_eq!(
            set_with(never, Some("Bad"), None).unwrap_err(),
            PROVIDER_INVALID
        );
        assert_eq!(
            set_with(never, None, Some("a b")).unwrap_err(),
            MODEL_INVALID
        );
        assert_eq!(status_with(None)["installed"], false);
    }

    /// A stand-in `hermes` records each call and answers `config get model --json` from what it
    /// was told to set, so the order and the arguments are what a real run would send.
    #[cfg(unix)]
    #[test]
    fn a_change_runs_hermes_config_set_provider_first_then_reads_back() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("calls");
        let program = dir.path().join("hermes");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\nif [ \"$2\" = get ]; then printf '{{\"provider\":\"anthropic\",\"default\":\"claude-sonnet-4.5\"}}'; fi\n",
                log = log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let status = set_with(&program, Some("anthropic"), Some("claude-sonnet-4.5")).unwrap();
        assert_eq!(status["provider"], "anthropic");
        assert_eq!(status["model"], "claude-sonnet-4.5");
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "config set model.provider anthropic\nconfig set model.default claude-sonnet-4.5\nconfig get model --json\n"
        );
        // A failing Hermes is named, not mistaken for success.
        std::fs::write(&program, "#!/bin/sh\nexit 2\n").unwrap();
        assert_eq!(
            set_with(&program, None, Some("a/b")).unwrap_err(),
            HERMES_FAILED
        );
        assert_eq!(status_with(Some(&program))["error"], HERMES_FAILED);
    }
}
