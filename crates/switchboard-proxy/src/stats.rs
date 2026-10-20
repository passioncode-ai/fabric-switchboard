//! SB-145 (operator 2026-10-19: «отдельный экран статистики … куда токены уходят»): what the
//! proxy relays, counted per day, account, pool and client. Only counts leave a request: the
//! number of requests and their outcome, and the token counts the provider reports in its own
//! response (`usage`). No prompt, answer, path or header value is kept.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    pub requests: u64,
    pub ok: u64,
    pub limited: u64,
    pub failed: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}
impl Tally {
    pub fn add(&mut self, other: &Tally) {
        self.requests += other.requests;
        self.ok += other.ok;
        self.limited += other.limited;
        self.failed += other.failed;
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
    }
}

/// `day\taccount\tpool\tclient\tfolder` → its tally. Day is the UTC date (`YYYY-MM-DD`);
/// folder is the group folder the request came from (SB-147), empty when it is not known.
pub type Tallies = BTreeMap<String, Tally>;
pub type Stats = Arc<Mutex<Tallies>>;

pub fn key(day: &str, account: &str, pool: &str, client: &str, folder: &str) -> String {
    format!("{day}\t{account}\t{pool}\t{client}\t{folder}")
}
/// One key's parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parts<'a> {
    pub day: &'a str,
    pub account: &'a str,
    pub pool: &'a str,
    pub client: &'a str,
    pub folder: &'a str,
}
/// The parts of a key; a key from 0.6.52 (four fields) has no folder.
pub fn parts(key: &str) -> Option<Parts<'_>> {
    let f: Vec<&str> = key.split('\t').collect();
    match f.as_slice() {
        [day, account, pool, client] => Some(Parts {
            day,
            account,
            pool,
            client,
            folder: "",
        }),
        [day, account, pool, client, folder] => Some(Parts {
            day,
            account,
            pool,
            client,
            folder,
        }),
        _ => None,
    }
}

/// The folder a request names (SB-147): the newest `<cwd>` of a Codex environment context, or
/// Claude Code's `Working directory:` line in its first mebibyte. A byte scan, never a parse of
/// the whole body; only an absolute path is taken.
pub fn folder_hint(body: &[u8]) -> Option<String> {
    let cwd = rfind(body, b"<cwd>").and_then(|at| {
        let rest = &body[at + 5..];
        find(rest, b"</cwd>").map(|end| &rest[..end])
    });
    let raw = cwd.or_else(|| {
        let head = &body[..body.len().min(1024 * 1024)];
        find(head, b"Working directory: ").map(|at| {
            let rest = &head[at + 19..];
            let end = rest
                .iter()
                .position(|b| matches!(b, b'\\' | b'"' | b'\n' | b'\r' | b'<'))
                .unwrap_or(rest.len().min(4096));
            &rest[..end]
        })
    })?;
    let text = std::str::from_utf8(raw).ok()?.trim().replace("\\/", "/");
    let absolute = text.starts_with('/') || text.as_bytes().get(1) == Some(&b':');
    (absolute && text.len() <= 4096).then_some(text)
}
/// The deepest of `folders` that holds `path` (by path segments), if any.
pub fn deepest<'a>(folders: &'a [String], path: &str) -> Option<&'a str> {
    let p = std::path::Path::new(path);
    folders
        .iter()
        .filter(|f| p.starts_with(f.as_str()))
        .max_by_key(|f| f.len())
        .map(String::as_str)
}
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// The UTC date of a Unix time.
pub fn day_of(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", y + i64::from(m <= 2), m, d)
}

/// Which coding agent sent a request, from its `user-agent`: a short family name, never the
/// header itself.
pub fn client_of(user_agent: Option<&str>) -> String {
    let ua = user_agent.unwrap_or_default().to_ascii_lowercase();
    for (needle, name) in [
        ("claude-cli", "claude-code"),
        ("claude-code", "claude-code"),
        ("codex", "codex"),
        ("opencode", "opencode"),
        ("hermes", "hermes"),
        ("kilo", "kilo"),
        ("goose", "goose"),
        ("aider", "aider"),
        ("crush", "crush"),
        ("qwen", "qwen-code"),
        ("kimi", "kimi"),
        ("pi-coding", "pi"),
        ("anthropic/", "anthropic-sdk"),
        ("openai/", "openai-sdk"),
    ] {
        if ua.contains(needle) {
            return name.into();
        }
    }
    "other".into()
}

/// Reads the provider's own token counts out of a response as it streams by: Anthropic's
/// `message_start` / `message_delta` events or a `usage` object, OpenAI's `response.completed`
/// event or a `usage` object. Bounded: a line longer than `LINE_LIMIT` and a body past
/// `BODY_LIMIT` are not read.
#[derive(Default)]
pub struct UsageScanner {
    line: Vec<u8>,
    body: Vec<u8>,
    sse: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}
const LINE_LIMIT: usize = 1024 * 1024;
const BODY_LIMIT: usize = 4 * 1024 * 1024;
impl UsageScanner {
    pub fn feed(&mut self, chunk: &[u8]) {
        if self.body.len() + chunk.len() <= BODY_LIMIT {
            self.body.extend_from_slice(chunk);
        } else {
            self.body.clear();
            self.body.shrink_to_fit();
            self.body.push(0);
        }
        for &b in chunk {
            if b == b'\n' {
                let line = std::mem::take(&mut self.line);
                self.event_line(&line);
            } else if self.line.len() < LINE_LIMIT {
                self.line.push(b);
            }
        }
    }
    fn event_line(&mut self, line: &[u8]) {
        let Some(data) = line.strip_prefix(b"data:") else {
            return;
        };
        self.sse = true;
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(trim(data)) else {
            return;
        };
        match v["type"].as_str() {
            Some("message_start") => self.usage(&v["message"]["usage"]),
            Some("message_delta") => {
                // Cumulative: the last one is the turn's output.
                if let Some(out) = v["usage"]["output_tokens"].as_u64() {
                    self.output_tokens = out;
                }
            }
            Some("response.completed") => self.usage(&v["response"]["usage"]),
            _ => {}
        }
    }
    fn usage(&mut self, u: &serde_json::Value) {
        let n = |k: &str| u[k].as_u64();
        if let Some(i) = n("input_tokens").or_else(|| n("prompt_tokens")) {
            self.input_tokens = i;
        }
        if let Some(o) = n("output_tokens").or_else(|| n("completion_tokens")) {
            self.output_tokens = o;
        }
        if let Some(c) = n("cache_read_input_tokens")
            .or_else(|| u["input_tokens_details"]["cached_tokens"].as_u64())
            .or_else(|| u["prompt_tokens_details"]["cached_tokens"].as_u64())
        {
            self.cache_read_tokens = c;
        }
        if let Some(c) = n("cache_creation_input_tokens") {
            self.cache_write_tokens = c;
        }
    }
    /// At the end of the body: a plain JSON answer (not a stream) gives its `usage` here.
    pub fn finish(&mut self) {
        if !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.event_line(&line);
        }
        if !self.sse && self.body.first() != Some(&0) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&self.body) {
                self.usage(&v["usage"]);
            }
        }
        self.body = Vec::new();
    }
}
fn trim(b: &[u8]) -> &[u8] {
    let start = b
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(b.len());
    let end = b
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(start, |e| e + 1);
    &b[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_counts_come_from_the_providers_own_usage_and_nothing_else() {
        let mut a = UsageScanner::default();
        // Split mid-line on purpose: a chunk boundary is not an event boundary.
        a.feed(b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":120,\"cache_read_input_tokens\":4000,\"cache_creation_input_tokens\":50,\"output_tokens\":1}}}\n\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi\"}}\n\ndata: {\"type\":\"message_de");
        a.feed(b"lta\",\"usage\":{\"output_tokens\":37}}\n\n");
        a.finish();
        assert_eq!(
            (
                a.input_tokens,
                a.output_tokens,
                a.cache_read_tokens,
                a.cache_write_tokens
            ),
            (120, 37, 4000, 50)
        );
        let mut o = UsageScanner::default();
        o.feed(b"data: {\"type\":\"response.created\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":900,\"input_tokens_details\":{\"cached_tokens\":800},\"output_tokens\":60}}}\n\n");
        o.finish();
        assert_eq!(
            (o.input_tokens, o.output_tokens, o.cache_read_tokens),
            (900, 60, 800)
        );
        let mut plain = UsageScanner::default();
        plain.feed(br#"{"id":"x","usage":{"prompt_tokens":10,"completion_tokens":3}}"#);
        plain.finish();
        assert_eq!((plain.input_tokens, plain.output_tokens), (10, 3));
        let mut none = UsageScanner::default();
        none.feed(b"not json at all");
        none.finish();
        assert_eq!((none.input_tokens, none.output_tokens), (0, 0));
    }

    #[test]
    fn days_clients_and_keys() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(1_792_400_000), "2026-10-19");
        assert_eq!(day_of(951_782_400), "2000-02-29");
        assert_eq!(
            client_of(Some("claude-cli/2.1.300 (external, cli)")),
            "claude-code"
        );
        assert_eq!(
            client_of(Some("codex_cli_rs/0.163.0 (Mac OS 26.0)")),
            "codex"
        );
        assert_eq!(client_of(None), "other");
        let k = key("2026-10-19", "acc", "pool", "codex", "/w/app");
        let p = parts(&k).unwrap();
        assert_eq!(
            (p.day, p.account, p.pool, p.client, p.folder),
            ("2026-10-19", "acc", "pool", "codex", "/w/app")
        );
        assert_eq!(
            parts("2026-10-19\tacc\tpool\tcodex").unwrap().folder,
            "",
            "a 0.6.52 key"
        );
        assert_eq!(parts("a\tb"), None);
    }

    #[test]
    fn a_request_names_its_folder_without_its_body_being_kept() {
        let claude = br#"{"system":[{"type":"text","text":"...<env>\nWorking directory: /Users/me/DATA/app/src\nIs directory a git repo: Yes"}],"messages":[]}"#;
        assert_eq!(
            folder_hint(claude).as_deref(),
            Some("/Users/me/DATA/app/src")
        );
        let codex = br#"{"input":[{"content":"<environment_context><cwd>/w/old</cwd></environment_context>"},{"content":"<environment_context><cwd>/w/new</cwd>"}]}"#;
        assert_eq!(
            folder_hint(codex).as_deref(),
            Some("/w/new"),
            "the newest context"
        );
        assert_eq!(folder_hint(br#"{"messages":[]}"#), None);
        assert_eq!(folder_hint(br#"Working directory: relative/path\n"#), None);
        let folders = vec![
            "/w/app".to_string(),
            "/w/app/sub".to_string(),
            "/w/other".to_string(),
        ];
        assert_eq!(deepest(&folders, "/w/app/sub/x"), Some("/w/app/sub"));
        assert_eq!(deepest(&folders, "/w/apple"), None, "by path segments");
    }
}
