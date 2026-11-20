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
/// Why a run of Hermes gave no answer (SB-583, board request be4e4cf4): each says which, so a
/// Hermes that works in a terminal but not from the app can be told apart. Before, all five read
/// as `HERMES_FAILED`.
pub const HERMES_NOT_STARTED: &str =
    "Hermes could not be started. Check that the hermes command runs in a terminal, then try again.";
pub const HERMES_TIMED_OUT: &str =
    "Hermes did not answer within 15 seconds. Run hermes config check, then try again.";
pub const HERMES_EXITED: &str =
    "Hermes ended with an error. Run hermes config check, then try again.";
pub const HERMES_UNREADABLE: &str =
    "Hermes answered with something Switchboard cannot read. Run hermes config get model --json to see it.";
pub const PROVIDER_INVALID: &str =
    "A Hermes provider is an id such as openrouter, anthropic or nous (lowercase letters, digits and - . _ :).";
pub const MODEL_INVALID: &str =
    "A model is an id such as moonshotai/kimi-k2 or claude-sonnet-4.5 (letters, digits and / . _ : -).";
pub const NOTHING_TO_SET: &str = "Name a provider, a model or both.";
/// The account a continuation names to run on Hermes Agent with the saved OpenRouter key and its
/// model (SB-107): `switchboard continue <wf> --account hermes`, and a chain's `hermes` step.
pub const HERMES_TARGET: &str = "hermes";

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

/// Why a run of Hermes gave no answer (SB-583).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The program could not be started (gone since it was found, not executable).
    NotStarted,
    /// No exit within the deadline: Hermes and everything it started were killed.
    TimedOut(Duration),
    /// It exited unsuccessfully; the code, unless a signal ended it.
    Exited(Option<i32>),
    /// It answered, but more than 64 KiB, not UTF-8, or not JSON.
    Unreadable,
}
impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Failure::NotStarted => HERMES_NOT_STARTED,
            Failure::TimedOut(_) => HERMES_TIMED_OUT,
            Failure::Exited(_) => HERMES_EXITED,
            Failure::Unreadable => HERMES_UNREADABLE,
        }
    }
    pub fn kind(self) -> &'static str {
        match self {
            Failure::NotStarted => "not_started",
            Failure::TimedOut(_) => "timed_out",
            Failure::Exited(_) => "exited",
            Failure::Unreadable => "unreadable",
        }
    }
    /// `error`, `error_kind` and the figure that goes with it (`timeout_secs`, `exit_code`).
    fn describe(self, status: &mut Value) {
        status["error"] = json!(self.message());
        status["error_kind"] = json!(self.kind());
        match self {
            Failure::TimedOut(limit) => status["timeout_secs"] = json!(limit.as_secs()),
            Failure::Exited(code) => status["exit_code"] = json!(code),
            _ => {}
        }
    }
}

/// Runs Hermes with fixed arguments and a deadline; stdout only, bounded. Never through a shell.
fn run(program: &Path, args: &[&str]) -> Result<String, Failure> {
    run_within(program, args, TIMEOUT)
}
/// `run` with its own deadline. Hermes runs in its own process group: at the deadline the group is
/// killed (a plugin or helper it started dies with it, LC-02) and reaped; stdout is read on its own
/// thread, so a full pipe cannot hold the deadline up.
fn run_within(program: &Path, args: &[&str], timeout: Duration) -> Result<String, Failure> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(|_| Failure::NotStarted)?;
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(stdout) = stdout {
            let _ = stdout.take(MAX_OUTPUT as u64 + 1).read_to_end(&mut out);
        }
        out
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                #[cfg(unix)]
                if let Ok(group) = libc::pid_t::try_from(child.id()) {
                    // SAFETY: killpg only signals the group this call created.
                    unsafe {
                        libc::killpg(group, libc::SIGKILL);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    match status {
        None => Err(Failure::TimedOut(timeout)),
        Some(status) if !status.success() => Err(Failure::Exited(status.code())),
        Some(_) if out.len() > MAX_OUTPUT => Err(Failure::Unreadable),
        Some(_) => String::from_utf8(out).map_err(|_| Failure::Unreadable),
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

/// The JSON Hermes printed: the whole of stdout, or else its last non-empty line (a line a plugin
/// or helper printed to stdout before it). `None` when neither is JSON; empty output is `Null`
/// (a home without a model section).
fn model_json(text: &str) -> Option<Value> {
    let text = text.trim();
    if text.is_empty() {
        return Some(Value::Null);
    }
    serde_json::from_str(text).ok().or_else(|| {
        text.lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .and_then(|line| serde_json::from_str(line).ok())
    })
}

/// `{model, provider, base_url, api_mode}` from Hermes's resolved `model` section; anything else
/// in it (keys, tokens) is dropped.
pub fn parse_model(text: &str) -> Value {
    let value = model_json(text).unwrap_or(Value::Null);
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

fn failed(failure: Failure) -> Value {
    let mut status = json!({"configured": false});
    failure.describe(&mut status);
    status
}

fn status_with(program: Option<&Path>) -> Value {
    let Some(program) = program else {
        return json!({"installed": false, "configured": false, "error": null, "providers": COMMON_PROVIDERS});
    };
    let mut status = match run(program, &["config", "get", "model", "--json"]) {
        Ok(text) if model_json(&text).is_some() => parse_model(&text),
        Ok(_) => failed(Failure::Unreadable),
        Err(failure) => failed(failure),
    };
    status["installed"] = json!(true);
    status["providers"] = json!(COMMON_PROVIDERS);
    if status.get("error").is_none() {
        status["error"] = Value::Null;
    }
    status
}

/// The model the ordinary Hermes is set to when it runs on OpenRouter (SB-107, operator
/// 2026-10-12): a continuation on the saved OpenRouter key keeps the person's own choice, and only
/// a Hermes on another provider (or none) falls back to the key's default model.
///
/// Read at a hand-over, so it waits `MODEL_TIMEOUT` at most: a Hermes that does not answer gives
/// no model, and the key's default model is used.
pub fn openrouter_model() -> Option<String> {
    let program = find_program("hermes")?;
    let text = run_within(
        &program,
        &["config", "get", "model", "--json"],
        MODEL_TIMEOUT,
    )
    .ok()?;
    own_openrouter_model(&parse_model(&text))
}
const MODEL_TIMEOUT: Duration = Duration::from_secs(5);
fn own_openrouter_model(status: &Value) -> Option<String> {
    (status.get("provider").and_then(Value::as_str) == Some("openrouter"))
        .then(|| status.get("model").and_then(Value::as_str))
        .flatten()
        .filter(|m| switchboard_core::agent_keys::model_valid(m))
        .map(str::to_owned)
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
        run(program, &["config", "set", "model.provider", provider]).map_err(Failure::message)?;
    }
    if let Some(model) = model {
        run(program, &["config", "set", "model.default", model]).map_err(Failure::message)?;
    }
    Ok(status_with(Some(program)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_hermes_that_hangs_is_cut_off_with_everything_it_started() {
        // A helper still holding stdout would keep the reader waiting without the group kill.
        let started = Instant::now();
        let answer = run_within(
            Path::new("/bin/sh"),
            &["-c", "sleep 30 & echo partial; sleep 30"],
            Duration::from_millis(300),
        );
        assert_eq!(answer, Err(Failure::TimedOut(Duration::from_millis(300))));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            run_within(Path::new("/bin/sh"), &["-c", "echo '{}'"], TIMEOUT).as_deref(),
            Ok("{}\n")
        );
    }
    #[test]
    fn a_continuation_keeps_the_model_hermes_runs_on_openrouter() {
        let own = |text: &str| own_openrouter_model(&parse_model(text));
        assert_eq!(
            own(r#"{"default": "deepseek/deepseek-v4-pro-0813", "provider": "openrouter"}"#)
                .as_deref(),
            Some("deepseek/deepseek-v4-pro-0813")
        );
        // Another provider's model means nothing on an OpenRouter key; none set, none kept.
        assert_eq!(own(r#"{"default": "hermes-4", "provider": "nous"}"#), None);
        assert_eq!(own(""), None);
        assert_eq!(
            own(r#"{"default": "bad model; rm", "provider": "openrouter"}"#),
            None
        );
    }
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
            HERMES_EXITED
        );
        let status = status_with(Some(&program));
        assert_eq!(status["error"], HERMES_EXITED);
        assert_eq!(status["error_kind"], "exited");
        assert_eq!(status["exit_code"], 2);
    }

    /// SB-583 (board request be4e4cf4): each way the probe fails says which, with its figure; a
    /// line printed before the JSON does not hide the answer; stderr is never read.
    #[cfg(unix)]
    #[test]
    fn the_probe_names_why_hermes_gave_no_answer() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("hermes");
        let answer = |body: &str| {
            std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            status_with(Some(&program))
        };
        let status = answer("echo 'Command helper: applied 3 secrets' >&2\necho 'banner'\necho '{\"provider\": \"openrouter\", \"default\": \"a/b\"}'");
        assert_eq!(status["error"], Value::Null, "{status}");
        assert_eq!(status["provider"], "openrouter");
        assert_eq!(status["model"], "a/b");
        assert!(status.get("error_kind").is_none());

        let status = answer("echo 'not json'");
        assert_eq!(status["error"], HERMES_UNREADABLE);
        assert_eq!(status["error_kind"], "unreadable");
        assert_eq!(status["installed"], true);

        // Empty output is a home without a model section, not a failure.
        let status = answer("exit 0");
        assert_eq!(status["error"], Value::Null);
        assert_eq!(status["configured"], false);

        let status = answer("kill -9 $$");
        assert_eq!(status["error_kind"], "exited");
        assert_eq!(status["exit_code"], Value::Null);

        let gone = dir.path().join("gone");
        let status = status_with(Some(&gone));
        assert_eq!(status["error"], HERMES_NOT_STARTED);
        assert_eq!(status["error_kind"], "not_started");

        let mut timed_out = json!({});
        Failure::TimedOut(TIMEOUT).describe(&mut timed_out);
        assert_eq!(timed_out["error"], HERMES_TIMED_OUT);
        assert_eq!(timed_out["error_kind"], "timed_out");
        assert_eq!(timed_out["timeout_secs"], 15);
        assert!(HERMES_TIMED_OUT.contains(&format!("{} seconds", TIMEOUT.as_secs())));
    }
}
