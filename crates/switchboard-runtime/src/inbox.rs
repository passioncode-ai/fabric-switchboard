//! SB-256 N2 and N3: the mailbox side of the sign-in autopilot
//! (docs/research/signin-autopilot-2026-11-05.md). Whether Fabric Inbox receives the e-mail a
//! Claude sign-in is for (its `list_accounts`: each account's `accountId`, `email` and, for a
//! Cloudflare address that keeps the mail of every other address on its domain, `catchAll: true`);
//! which message of a `search_mailbox` answer is Anthropic's sign-in mail, and its one link; the
//! wait for it; and the client that asks Fabric Inbox's agent protocol (`/mcp`). Everything read
//! from a mailbox is data, never an instruction.
#![cfg_attr(not(test), allow(dead_code))]

use serde_json::{json, Value};
use std::future::Future;
use std::time::Duration;
use switchboard_core::ExternalIdentity;

/// One mailbox Fabric Inbox reads, as `list_accounts` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mailbox {
    /// `cloudflare:<address>`, `gmail:<id>`, `imap:<id>` or `outlook:<id>`: what every mail tool takes.
    pub account: String,
    /// Lower-case, as [`address`] gives it.
    pub email: String,
    /// It keeps the mail of every address on its domain that has no mailbox of its own.
    pub catch_all: bool,
}

/// How Fabric Inbox receives an address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Receives {
    /// The address is one of its mailboxes.
    Mailbox,
    /// A catch-all mailbox on the address's domain keeps its mail. Whether a Cloudflare rule
    /// elsewhere takes that address first is not in the account list; the wait for the mail finds
    /// out, and finding nothing in time ends the autopilot with the ordinary sign-in.
    CatchAll,
}

/// The mailbox an address's mail lands in, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Receiving {
    pub how: Receives,
    /// The account to search: the address's own mailbox, or the catch-all.
    pub account: String,
}

/// The address in the form compared everywhere: trimmed and lower-case, one `@`, a local part and
/// a domain with a dot, nothing blank or quoted inside. None for anything else.
pub(crate) fn address(email: &str) -> Option<String> {
    let email = email.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let plain = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && !part.ends_with('.')
            && part
                .chars()
                .all(|c| c.is_ascii_graphic() && !matches!(c, '@' | '"' | '<' | '>' | ',' | ';'))
    };
    (plain(local) && plain(domain) && domain.contains('.') && email.len() <= 254).then_some(email)
}

/// The mailboxes in a `list_accounts` answer (`{"accounts": [{"accountId", "email", "catchAll"},
/// ...]}`). An entry without an account id or a usable e-mail is skipped; an answer that is not
/// such a list gives none.
pub(crate) fn mailboxes(answer: &Value) -> Vec<Mailbox> {
    let Some(accounts) = answer.get("accounts").and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    accounts
        .iter()
        .filter_map(|entry| {
            let account = entry.get("accountId")?.as_str()?;
            if account.is_empty() || account.len() > 320 {
                return None;
            }
            let email = address(entry.get("email")?.as_str()?)?;
            let catch_all = entry
                .get("catchAll")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(Mailbox {
                account: account.to_owned(),
                email,
                catch_all,
            })
        })
        .collect()
}

/// Whether Fabric Inbox receives `email`, and in which mailbox: the mailbox that is that address,
/// else a catch-all mailbox on its domain. None for an address it does not receive, or one that
/// is not an address.
pub(crate) fn receives(email: &str, mailboxes: &[Mailbox]) -> Option<Receiving> {
    let email = address(email)?;
    if let Some(m) = mailboxes.iter().find(|m| m.email == email) {
        return Some(Receiving {
            how: Receives::Mailbox,
            account: m.account.clone(),
        });
    }
    let domain = email.split_once('@')?.1;
    mailboxes
        .iter()
        .find(|m| m.catch_all && m.email.split_once('@').is_some_and(|(_, d)| d == domain))
        .map(|m| Receiving {
            how: Receives::CatchAll,
            account: m.account.clone(),
        })
}

/// The e-mail a sign-in again is for: the one the provider gave when the account was captured.
/// A new account has none; the person types it.
pub(crate) fn sign_in_email(identity: Option<&ExternalIdentity>) -> Option<String> {
    address(identity?.email.as_deref()?)
}

/// The domains Anthropic's sign-in mail comes from: the address's domain is one of these or ends
/// in `.` and one of these. Mail from anywhere else is never read for a link.
const SENDER_DOMAINS: &[&str] = &["anthropic.com", "claude.ai", "claude.com"];
/// The hosts a sign-in link may lead to (SB-256 design, step 4), each with its subdomains.
const LINK_HOSTS: &[&str] = &["claude.ai", "claude.com"];
/// Pages Anthropic's mail links to that are never a sign-in.
const NOT_SIGN_IN: &[&str] = &["/legal", "/privacy", "/terms", "/support", "/unsubscribe"];

fn under(host: &str, domains: &[&str]) -> bool {
    domains
        .iter()
        .any(|d| host == *d || host.strip_suffix(d).is_some_and(|rest| rest.ends_with('.')))
}

/// The address inside a header such as `Claude <no-reply@mail.anthropic.com>`, or the header
/// itself when it is a plain address.
fn header_address(header: &str) -> Option<String> {
    let inner = match (header.rfind('<'), header.rfind('>')) {
        (Some(open), Some(close)) if open < close => &header[open + 1..close],
        _ => header,
    };
    address(inner)
}

/// When a message arrived: RFC 3339 (`2026-11-05T19:00:00Z`) or RFC 2822 (`Thu, 05 Nov 2026
/// 19:00:00 +0000`), as Fabric Inbox's providers write it; seconds since the epoch.
fn arrived(date: &str) -> Option<i64> {
    use time::format_description::well_known::{Rfc2822, Rfc3339};
    let date = date.trim();
    time::OffsetDateTime::parse(date, &Rfc3339)
        .or_else(|_| time::OffsetDateTime::parse(date, &Rfc2822))
        .ok()
        .map(|t| t.unix_timestamp())
}

/// The id of the newest sign-in message in a `search_mailbox` answer (`{"messages": [{"messageId",
/// "from", "to", "date"}, ...]}`, the same for every kind of mailbox): one to `email`, from one of
/// Anthropic's domains, that arrived at or after `since` (seconds since the epoch, the moment the
/// browser asked for the mail). A row with an unreadable date never counts.
pub(crate) fn sign_in_message(answer: &Value, email: &str, since: i64) -> Option<String> {
    let email = address(email)?;
    let rows = answer.get("messages")?.as_array()?;
    rows.iter()
        .filter_map(|row| {
            let text = |key: &str| row.get(key).and_then(|v| v.as_str());
            let sender = header_address(text("from")?)?;
            let domain = sender.split_once('@')?.1.to_owned();
            let to_us = text("to")?
                .split(',')
                .filter_map(header_address)
                .any(|r| r == email);
            let at = arrived(text("date")?)?;
            let id = text("messageId").filter(|id| !id.is_empty() && id.len() <= 300)?;
            (under(&domain, SENDER_DOMAINS) && to_us && at >= since).then(|| (at, id.to_owned()))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, id)| id)
}

/// Why a message gave no sign-in link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoLink {
    /// No `https` link to a sign-in host.
    None,
    /// More than one different link to a sign-in host: none is guessed.
    Several,
}

/// The one sign-in link in a message's text and HTML: `https`, on claude.ai or claude.com (or a
/// subdomain), not a legal, privacy, terms, support or unsubscribe page. The same link written
/// twice (text and HTML, `&amp;` in HTML) counts once; two different ones are refused, and the
/// mail's words are never followed.
pub(crate) fn sign_in_link(text: &str, html: Option<&str>) -> Result<String, NoLink> {
    let pattern = regex::Regex::new(r#"https://[^\s"'<>()\]]+"#).expect("a fixed pattern");
    let mut found: Vec<String> = Vec::new();
    for source in [text, html.unwrap_or("")] {
        for m in pattern.find_iter(source) {
            let link = m
                .as_str()
                .replace("&amp;", "&")
                .trim_end_matches(['.', ',', ';', ':'])
                .to_owned();
            let rest = &link["https://".len()..];
            let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
            let host = rest[..end].to_lowercase();
            let path = rest[end..].split(['?', '#']).next().unwrap_or("");
            if host.contains(['@', ':']) || !under(&host, LINK_HOSTS) {
                continue;
            }
            if path.is_empty() || path == "/" || NOT_SIGN_IN.iter().any(|p| path.starts_with(p)) {
                continue;
            }
            if !found.contains(&link) {
                found.push(link);
            }
        }
    }
    match found.len() {
        0 => Err(NoLink::None),
        1 => Ok(found.remove(0)),
        _ => Err(NoLink::Several),
    }
}

/// How often the mailbox is searched while a sign-in waits for its mail, and for how long (SB-256
/// design, step 4).
pub(crate) const POLL_EVERY: Duration = Duration::from_secs(5);
pub(crate) const POLL_FOR: Duration = Duration::from_secs(300);

/// What the sign-in reads of a mailbox: [`Client`], a fake in tests. Errors are short codes, never
/// a message's text.
pub(crate) trait Reader {
    /// A search of `account` for mail to `email` from the day `since` falls on (`search_mailbox`).
    fn search(
        &self,
        account: &str,
        email: &str,
        since: i64,
    ) -> impl Future<Output = Result<Value, String>> + Send;
    /// One message's text and HTML (`read_message` with `includeHtml`).
    fn read(
        &self,
        account: &str,
        id: &str,
    ) -> impl Future<Output = Result<(String, Option<String>), String>> + Send;
}

/// How waiting for the sign-in mail ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Waited {
    /// The one sign-in link of the mail.
    Link(String),
    /// The mail came without one usable link; a person signs in.
    NoLink(NoLink),
    /// No mail in time; the last error the mailbox gave, if every look failed.
    TimedOut(Option<String>),
}

/// Searches `account` every `every` until Anthropic's sign-in mail to `email`, sent at or after
/// `since` (seconds since the epoch), is there, then takes its link. A failed search or read is
/// tried again at the next look; after `give_up` it times out. `elapsed` and `sleep` are the
/// clock, so a test runs it without waiting.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn wait_for_link<R: Reader, S: Future<Output = ()>>(
    reader: &R,
    account: &str,
    email: &str,
    since: i64,
    every: Duration,
    give_up: Duration,
    elapsed: impl Fn() -> Duration,
    sleep: impl Fn(Duration) -> S,
) -> Waited {
    let mut last_error = None;
    let mut found_any = false;
    loop {
        match reader.search(account, email, since).await {
            Ok(answer) => {
                found_any = true;
                if let Some(id) = sign_in_message(&answer, email, since) {
                    match reader.read(account, &id).await {
                        Ok((text, html)) => {
                            return match sign_in_link(&text, html.as_deref()) {
                                Ok(link) => Waited::Link(link),
                                Err(why) => Waited::NoLink(why),
                            };
                        }
                        Err(e) => last_error = Some(e),
                    }
                }
            }
            Err(e) => last_error = Some(e),
        }
        if elapsed() + every > give_up {
            return Waited::TimedOut(if found_any { None } else { last_error });
        }
        sleep(every).await;
    }
}

/// The server did not answer, or answered something that is not the protocol.
pub(crate) const INBOX_UNREACHABLE: &str = "inbox_unreachable";
/// The key was refused (401 or 403, or a redirect to a sign-in, which is never followed).
pub(crate) const INBOX_REFUSED: &str = "inbox_refused";
/// The tool answered with an error (a mailbox the key may not read, a bad argument).
pub(crate) const INBOX_FAILED: &str = "inbox_failed";
const REQUEST: Duration = Duration::from_secs(15);
const MAX_ANSWER: usize = 4 * 1024 * 1024;

/// A Fabric Inbox server's origin as the client takes it: `https://<host>[:<port>]` with nothing
/// after it but an optional `/`, or a test stand-in on `http://127.0.0.1:<port>`. None otherwise.
pub(crate) fn origin(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('/').to_lowercase();
    let digits = |p: &str| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit());
    if let Some(port) = text.strip_prefix("http://127.0.0.1:") {
        return digits(port).then_some(text);
    }
    let rest = text.strip_prefix("https://")?;
    let (host, port) = match rest.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (rest, None),
    };
    let host_ok = host.len() <= 253
        && host.contains('.')
        && !host.starts_with(['.', '-'])
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    (host_ok && port.is_none_or(digits)).then_some(text)
}

/// A key's Client Id or Client Secret as the client sends it: printable ASCII, no space.
fn token(text: &str) -> bool {
    !text.is_empty() && text.len() <= 1024 && text.bytes().all(|b| b.is_ascii_graphic())
}

/// Fabric Inbox's agent protocol (`<origin>/mcp`, Streamable HTTP, no session), called with a
/// key's Cloudflare Access service token. Only `list_accounts`, `search_mailbox` and
/// `read_message` are asked; redirects are never followed.
pub(crate) struct Client {
    endpoint: String,
    client_id: String,
    secret: String,
    http: reqwest::Client,
}

impl Client {
    pub(crate) fn new(origin_text: &str, client_id: &str, secret: &str) -> Option<Self> {
        let origin = origin(origin_text)?;
        if !token(client_id) || !token(secret) {
            return None;
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST)
            .build()
            .ok()?;
        Some(Self {
            endpoint: format!("{origin}/mcp"),
            client_id: client_id.to_owned(),
            secret: secret.to_owned(),
            http,
        })
    }

    /// One `tools/call`; the tool's answer as JSON (its `structuredContent`, else its first text).
    async fn call(&self, tool: &str, arguments: Value) -> Result<Value, String> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}});
        let response = self
            .http
            .post(&self.endpoint)
            .header("CF-Access-Client-Id", &self.client_id)
            .header("CF-Access-Client-Secret", &self.secret)
            .header("Accept", "application/json, text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|_| INBOX_UNREACHABLE.to_string())?;
        let status = response.status();
        if status.is_redirection() || status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(INBOX_REFUSED.into());
        }
        if !status.is_success() {
            return Err(INBOX_UNREACHABLE.into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| INBOX_UNREACHABLE.to_string())?;
        if bytes.len() > MAX_ANSWER {
            return Err(INBOX_UNREACHABLE.into());
        }
        let text = String::from_utf8_lossy(&bytes);
        // A server may answer as an event stream: its last `data:` line is the response.
        let json_text = if text.trim_start().starts_with('{') {
            text.to_string()
        } else {
            text.lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .next_back()
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        let reply: Value =
            serde_json::from_str(&json_text).map_err(|_| INBOX_UNREACHABLE.to_string())?;
        if reply.get("error").is_some() {
            return Err(INBOX_FAILED.into());
        }
        let result = reply.get("result").ok_or(INBOX_UNREACHABLE)?;
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            return Err(INBOX_FAILED.into());
        }
        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.clone());
        }
        result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .and_then(|t| serde_json::from_str(t).ok())
            .ok_or_else(|| INBOX_UNREACHABLE.to_string())
    }

    /// The mailboxes the key reads.
    pub(crate) async fn mailboxes(&self) -> Result<Vec<Mailbox>, String> {
        Ok(mailboxes(&self.call("list_accounts", json!({})).await?))
    }
}

/// The UTC day `seconds` falls on, as `search_mailbox`'s `after` takes it.
fn day(seconds: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .map(|t| t.date().to_string())
        .unwrap_or_else(|_| "1970-01-01".into())
}

impl Reader for Client {
    async fn search(&self, account: &str, email: &str, since: i64) -> Result<Value, String> {
        self.call(
            "search_mailbox",
            json!({"accountId": account, "to": email, "after": day(since), "limit": 20}),
        )
        .await
    }
    async fn read(&self, account: &str, id: &str) -> Result<(String, Option<String>), String> {
        let message = self
            .call(
                "read_message",
                json!({"accountId": account, "messageId": id, "includeHtml": true}),
            )
            .await?;
        let text = message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let html = message
            .get("html")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok((text, html))
    }
}

/// Where the key lives (design note, "Decision: where the Fabric Inbox key lives"): its Client
/// Secret is a vault item of Switchboard's own (`Store::put_secret`); this file (0600) beside the
/// store names that item, the server's origin, the Client Id and when it was saved, never the
/// secret. Only the sign-in autopilot reads the key.
const FILE: &str = "inbox.json";
/// Refusals; `person-errors.json` gives each its sentence.
pub const INBOX_ORIGIN_INVALID: &str = "Inbox origin invalid";
pub const INBOX_KEY_INVALID: &str = "Inbox key invalid";
pub const INBOX_NOT_SAVED: &str = "No inbox key saved";
pub const INBOX_UNREADABLE: &str = "Inbox key unreadable";
/// One save or removal at a time in this process, so a vault item is never orphaned.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    /// The vault item holding the Client Secret.
    item: String,
    origin: String,
    client_id: String,
    saved_at: i64,
}

fn link_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(FILE)
}
fn read_link(root: &std::path::Path) -> Result<Option<Link>, String> {
    let file = link_path(root);
    if std::fs::symlink_metadata(&file).is_err() {
        return Ok(None);
    }
    let bytes = switchboard_core::private_fs::read_private(&file, 16 * 1024)
        .map_err(|_| INBOX_UNREADABLE)?;
    let link: Link = serde_json::from_slice(&bytes).map_err(|_| INBOX_UNREADABLE)?;
    // A file edited by hand to point elsewhere is not used.
    if origin(&link.origin).as_deref() != Some(link.origin.as_str()) || !token(&link.client_id) {
        return Err(INBOX_UNREADABLE.into());
    }
    Ok(Some(link))
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Whether a key is saved, for which server and since when; never the Client Id or the secret.
pub fn status(root: &std::path::Path) -> Result<Value, String> {
    Ok(match read_link(root)? {
        Some(link) => json!({"saved": true, "origin": link.origin, "saved_at": link.saved_at}),
        None => json!({"saved": false, "origin": null, "saved_at": null}),
    })
}

/// Saves (or replaces) the key: the secret into the vault (the same item when one is saved), then
/// the file. A file that cannot be written leaves no new vault item behind.
pub fn set(
    root: &std::path::Path,
    store: &switchboard_core::Store,
    origin_text: &str,
    client_id: &str,
    secret: &str,
) -> Result<Value, String> {
    let origin = origin(origin_text).ok_or(INBOX_ORIGIN_INVALID)?;
    let (client_id, secret) = (client_id.trim(), secret.trim());
    if !token(client_id) || !token(secret) {
        return Err(INBOX_KEY_INVALID.into());
    }
    let _turn = TURN.lock().map_err(|_| INBOX_UNREADABLE)?;
    let before = read_link(root).ok().flatten();
    let item = store.put_secret(before.as_ref().map(|l| l.item.as_str()), secret)?;
    let link = Link {
        item: item.clone(),
        origin,
        client_id: client_id.to_owned(),
        saved_at: now(),
    };
    let written = serde_json::to_vec(&link)
        .map_err(|_| INBOX_UNREADABLE.to_string())
        .and_then(|bytes| switchboard_core::private_fs::private_write(&link_path(root), &bytes));
    if let Err(error) = written {
        if before.is_none() {
            let _ = store.delete_secret(&item);
        }
        return Err(error);
    }
    status(root)
}

/// Deletes the key: the vault item, then the file. A vault that refuses (a locked keychain) keeps
/// both, so no secret is left without its file.
pub fn remove(root: &std::path::Path, store: &switchboard_core::Store) -> Result<Value, String> {
    let _turn = TURN.lock().map_err(|_| INBOX_UNREADABLE)?;
    // An unreadable file names no item it can be trusted for: it alone goes.
    if let Some(link) = read_link(root).ok().flatten() {
        store.delete_secret(&link.item)?;
    }
    let existed = match std::fs::remove_file(link_path(root)) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err(INBOX_UNREADABLE.into()),
    };
    Ok(json!({"removed": existed}))
}

/// The vault item of the saved key, for `uninstall`.
pub(crate) fn vault_item(root: &std::path::Path) -> Option<String> {
    read_link(root).ok().flatten().map(|l| l.item)
}

/// A client with the saved key.
pub(crate) fn client(
    root: &std::path::Path,
    store: &switchboard_core::Store,
) -> Result<Client, String> {
    let link = read_link(root)?.ok_or(INBOX_NOT_SAVED)?;
    let secret = store.secret(&link.item)?;
    Client::new(&link.origin, &link.client_id, &secret).ok_or_else(|| INBOX_UNREADABLE.into())
}

/// [`status`], and with a saved key whether the server answers it: `answer` is `ok` with the
/// number of mailboxes it may read, or `refused`, `failed` or `unreachable`.
pub async fn check(
    root: &std::path::Path,
    store: &switchboard_core::Store,
) -> Result<Value, String> {
    let mut view = status(root)?;
    if view["saved"] != true {
        return Ok(view);
    }
    let client = client(root, store)?;
    match client.mailboxes().await {
        Ok(boxes) => {
            view["answer"] = json!("ok");
            view["mailboxes"] = json!(boxes.len());
        }
        Err(code) => {
            view["answer"] = json!(match code.as_str() {
                INBOX_REFUSED => "refused",
                INBOX_FAILED => "failed",
                _ => "unreachable",
            });
            view["mailboxes"] = Value::Null;
        }
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn list() -> Vec<Mailbox> {
        // As `list_accounts` answers: a Cloudflare address, a catch-all on another domain, a
        // hidden Gmail account, and entries without an account id or a usable e-mail.
        mailboxes(&json!({
            "accounts": [
                {"accountId": "cloudflare:signin@one.example", "provider": "cloudflare", "email": "SignIn@One.example", "hidden": false, "catchAll": false},
                {"accountId": "cloudflare:all@two.example", "provider": "cloudflare", "email": "all@two.example", "catchAll": true},
                {"accountId": "gmail:7", "provider": "gmail", "email": "person@mail.example", "hidden": true},
                {"provider": "gmail", "email": "noid@mail.example"},
                {"accountId": "gmail:8", "provider": "gmail"},
                {"accountId": "cloudflare:x", "provider": "cloudflare", "email": "not an address"}
            ]
        }))
    }
    fn mailbox(account: &str, email: &str, catch_all: bool) -> Mailbox {
        Mailbox {
            account: account.into(),
            email: email.into(),
            catch_all,
        }
    }

    #[test]
    fn the_account_list_gives_each_mailbox_with_its_account_and_catch_all() {
        assert_eq!(
            list(),
            vec![
                mailbox("cloudflare:signin@one.example", "signin@one.example", false),
                mailbox("cloudflare:all@two.example", "all@two.example", true),
                mailbox("gmail:7", "person@mail.example", false),
            ]
        );
        assert!(mailboxes(&json!({"accounts": "none"})).is_empty());
        assert!(mailboxes(&json!([])).is_empty());
    }

    #[test]
    fn an_address_is_received_by_its_mailbox_or_its_domains_catch_all_and_nothing_else() {
        let boxes = list();
        let at = |how, account: &str| {
            Some(Receiving {
                how,
                account: account.into(),
            })
        };
        assert_eq!(
            receives(" SIGNIN@one.example ", &boxes),
            at(Receives::Mailbox, "cloudflare:signin@one.example")
        );
        // A hidden mailbox still receives its mail.
        assert_eq!(
            receives("person@mail.example", &boxes),
            at(Receives::Mailbox, "gmail:7")
        );
        assert_eq!(
            receives("anyone@two.example", &boxes),
            at(Receives::CatchAll, "cloudflare:all@two.example")
        );
        // Another address on a domain without a catch-all, a subdomain, another domain.
        assert_eq!(receives("other@one.example", &boxes), None);
        assert_eq!(receives("anyone@sub.two.example", &boxes), None);
        assert_eq!(receives("person@gmail.example", &boxes), None);
        assert_eq!(receives("not an address", &boxes), None);
        assert_eq!(receives("signin@one.example", &[]), None);
    }

    #[test]
    fn only_a_plain_address_is_compared() {
        assert_eq!(address(" A@B.example "), Some("a@b.example".into()));
        for bad in [
            "",
            "a",
            "a@",
            "@b.example",
            "a@b",
            "a@@b.example",
            "a b@c.example",
            "\"a\"@b.example",
            "a@.b.example",
            "a.@b.example",
            "a@b.example.",
            "a@b.example,c@d.example",
        ] {
            assert_eq!(address(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_newest_sign_in_mail_to_the_address_since_the_request_is_chosen() {
        let since = arrived("2026-11-05T19:00:00Z").unwrap();
        // As `search_mailbox` answers, for every kind of mailbox.
        let answer = json!({"messages": [
            {"accountId": "a", "messageId": "old", "from": "Claude <no-reply@mail.anthropic.com>", "to": "signin@one.example", "date": "2026-11-05T18:59:59Z"},
            {"accountId": "a", "messageId": "first", "from": "Claude <no-reply@mail.anthropic.com>", "to": "signin@one.example", "date": "2026-11-05T19:00:10Z"},
            {"accountId": "a", "messageId": "newest", "from": "no-reply@anthropic.com", "to": "Other <x@one.example>, SignIn@One.example", "date": "Thu, 05 Nov 2026 19:01:00 +0000"},
            {"accountId": "a", "messageId": "spoof", "from": "Claude <no-reply@anthropic.com.evil.example>", "to": "signin@one.example", "date": "2026-11-05T19:05:00Z"},
            {"accountId": "a", "messageId": "elsewhere", "from": "no-reply@anthropic.com", "to": "other@one.example", "date": "2026-11-05T19:06:00Z"},
            {"accountId": "a", "messageId": "undated", "from": "no-reply@anthropic.com", "to": "signin@one.example", "date": "soon"},
            {"accountId": "a", "messageId": "", "from": "no-reply@claude.com", "to": "signin@one.example", "date": "2026-11-05T19:07:00Z"}
        ]});
        assert_eq!(
            sign_in_message(&answer, "signin@one.example", since),
            Some("newest".into())
        );
        assert_eq!(sign_in_message(&answer, "nobody@one.example", since), None);
        assert_eq!(
            sign_in_message(&json!({"messages": "x"}), "signin@one.example", since),
            None
        );
    }

    #[test]
    fn exactly_one_https_link_to_a_sign_in_host_is_taken() {
        let html = r#"<a href="https://claude.ai/magic-link?token=a&amp;b=1">Sign in</a>
            <a href="https://www.anthropic.com/legal/privacy">Privacy</a>
            <a href="https://claude.ai/legal/terms">Terms</a> <a href="https://claude.ai/">Claude</a>"#;
        let text = "Sign in: https://claude.ai/magic-link?token=a&b=1.";
        assert_eq!(
            sign_in_link(text, Some(html)),
            Ok("https://claude.ai/magic-link?token=a&b=1".into())
        );
        // Plain http, a look-alike host, credentials in the host: none of them.
        assert_eq!(
            sign_in_link(
                "http://claude.ai/x https://claude.ai.evil.example/x https://claude.ai@evil.example/x",
                None
            ),
            Err(NoLink::None)
        );
        assert_eq!(
            sign_in_link("https://claude.ai/a https://login.claude.com/b", None),
            Err(NoLink::Several)
        );
        assert_eq!(sign_in_link("", None), Err(NoLink::None));
    }

    type Body = Result<(String, Option<String>), String>;
    /// A mailbox that answers each look from a script: the n-th search gets the n-th answer (the
    /// last one again after that), and reads come from `bodies` in order.
    struct Script {
        searches: Vec<Result<Value, String>>,
        bodies: Vec<Body>,
        looks: Mutex<usize>,
        reads: Mutex<usize>,
    }
    impl Script {
        fn new(searches: Vec<Result<Value, String>>, bodies: Vec<Body>) -> Self {
            Self {
                searches,
                bodies,
                looks: Mutex::new(0),
                reads: Mutex::new(0),
            }
        }
    }
    impl Reader for Script {
        async fn search(&self, account: &str, _email: &str, _since: i64) -> Result<Value, String> {
            assert_eq!(account, "cloudflare:signin@one.example");
            let mut n = self.looks.lock().unwrap();
            *n += 1;
            self.searches[(*n - 1).min(self.searches.len() - 1)].clone()
        }
        async fn read(&self, _account: &str, id: &str) -> Result<(String, Option<String>), String> {
            assert_eq!(id, "m1");
            let mut n = self.reads.lock().unwrap();
            *n += 1;
            self.bodies[(*n - 1).min(self.bodies.len() - 1)].clone()
        }
    }
    async fn run(script: &Script) -> (Waited, Duration) {
        let clock = Mutex::new(Duration::ZERO);
        let waited = wait_for_link(
            script,
            "cloudflare:signin@one.example",
            "signin@one.example",
            arrived("2026-11-05T19:00:00Z").unwrap(),
            POLL_EVERY,
            POLL_FOR,
            || *clock.lock().unwrap(),
            |d| {
                *clock.lock().unwrap() += d;
                std::future::ready(())
            },
        )
        .await;
        let after = *clock.lock().unwrap();
        (waited, after)
    }
    fn mail() -> Value {
        json!({"messages": [{"accountId": "cloudflare:signin@one.example", "messageId": "m1", "from": "no-reply@mail.anthropic.com", "to": "signin@one.example", "date": "2026-11-05T19:00:30Z"}]})
    }

    #[tokio::test]
    async fn the_wait_takes_the_link_once_the_mail_arrives_and_retries_a_failed_look() {
        let script = Script::new(
            vec![
                Ok(json!({"messages": []})),
                Err(INBOX_UNREACHABLE.into()),
                Ok(mail()),
            ],
            vec![
                Err(INBOX_UNREACHABLE.into()),
                Ok(("Sign in: https://claude.ai/magic-link?t=1".into(), None)),
            ],
        );
        let (waited, after) = run(&script).await;
        assert_eq!(
            waited,
            Waited::Link("https://claude.ai/magic-link?t=1".into())
        );
        assert_eq!(
            *script.looks.lock().unwrap(),
            4,
            "the read that failed is tried at the next look"
        );
        assert_eq!(after, POLL_EVERY * 3);
    }

    #[tokio::test]
    async fn the_wait_ends_without_a_link_when_none_comes_in_time() {
        let empty = Script::new(vec![Ok(json!({"messages": []}))], vec![]);
        let (waited, after) = run(&empty).await;
        assert_eq!(waited, Waited::TimedOut(None));
        assert!(after <= POLL_FOR, "{after:?}");
        // A look at the start and one every POLL_EVERY up to and including POLL_FOR.
        assert_eq!(
            *empty.looks.lock().unwrap() as u64,
            POLL_FOR.as_secs() / POLL_EVERY.as_secs() + 1
        );
        // A mailbox that never answers names its last error.
        let down = Script::new(vec![Err(INBOX_REFUSED.into())], vec![]);
        assert_eq!(
            run(&down).await.0,
            Waited::TimedOut(Some(INBOX_REFUSED.into()))
        );
        // The mail with two different links stops at once for a person.
        let two = Script::new(
            vec![Ok(mail())],
            vec![Ok(("https://claude.ai/a https://claude.ai/b".into(), None))],
        );
        assert_eq!(
            run(&two).await,
            (Waited::NoLink(NoLink::Several), Duration::ZERO)
        );
    }

    #[test]
    fn only_an_https_origin_or_a_loopback_stand_in_is_a_server() {
        assert_eq!(
            origin(" HTTPS://Inbox.Example.com/ "),
            Some("https://inbox.example.com".into())
        );
        assert_eq!(
            origin("https://inbox.example.com:8443"),
            Some("https://inbox.example.com:8443".into())
        );
        assert_eq!(
            origin("http://127.0.0.1:4321"),
            Some("http://127.0.0.1:4321".into())
        );
        for bad in [
            "",
            "inbox.example.com",
            "http://inbox.example.com",
            "https://inbox.example.com/mcp",
            "https://user@inbox.example.com",
            "https://inbox.example.com:",
            "https://inbox.example.com:99999x",
            "https://localhost",
            "https://inbox.example.com?x=1",
            "http://127.0.0.1",
            "http://127.0.0.1:80/x",
        ] {
            assert_eq!(origin(bad), None, "{bad}");
        }
        assert!(Client::new("https://inbox.example.com", "id", "se cret").is_none());
        assert!(Client::new("https://inbox.example.com", "", "secret").is_none());
    }

    /// A stand-in for Fabric Inbox's `/mcp`: checks the key's headers, answers `list_accounts`,
    /// `search_mailbox` and `read_message`, and answers `status` for anything else.
    async fn stand_in(status: u16, stream: bool) -> String {
        use axum::{http::HeaderMap, routing::post, Json, Router};
        let app = Router::new().route(
            "/mcp",
            post(move |headers: HeaderMap, Json(body): Json<Value>| async move {
                let header = |n: &str| headers.get(n).and_then(|v| v.to_str().ok()).unwrap_or("");
                assert_eq!(header("cf-access-client-id"), "client.access");
                assert_eq!(header("cf-access-client-secret"), "fixture-secret");
                assert_eq!(body["method"], "tools/call");
                let args = &body["params"]["arguments"];
                let answer = match body["params"]["name"].as_str().unwrap() {
                    "list_accounts" => json!({"accounts": [{"accountId": "cloudflare:signin@one.example", "email": "signin@one.example", "catchAll": false}]}),
                    "search_mailbox" => {
                        assert_eq!(args["accountId"], "cloudflare:signin@one.example");
                        assert_eq!(args["to"], "signin@one.example");
                        assert_eq!(args["after"], "2026-11-05");
                        mail()
                    }
                    "read_message" => {
                        assert_eq!(args["includeHtml"], true);
                        json!({"messageId": "m1", "text": "Sign in", "html": "<a href=\"https://claude.ai/magic-link?t=1\">x</a>"})
                    }
                    _ => unreachable!(),
                };
                let reply = json!({"jsonrpc": "2.0", "id": 1, "result": {"content": [{"type": "text", "text": answer.to_string()}]}});
                let text = if stream {
                    format!("event: message\ndata: {reply}\n\n")
                } else {
                    reply.to_string()
                };
                (axum::http::StatusCode::from_u16(status).unwrap(), text)
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn the_client_lists_searches_and_reads_through_the_agent_protocol() {
        for stream in [false, true] {
            let server = stand_in(200, stream).await;
            let client = Client::new(&server, "client.access", "fixture-secret").unwrap();
            let boxes = client.mailboxes().await.unwrap();
            let at = receives("signin@one.example", &boxes).unwrap();
            let waited = wait_for_link(
                &client,
                &at.account,
                "signin@one.example",
                arrived("2026-11-05T19:00:00Z").unwrap(),
                POLL_EVERY,
                POLL_FOR,
                || Duration::ZERO,
                |_| std::future::ready(()),
            )
            .await;
            assert_eq!(
                waited,
                Waited::Link("https://claude.ai/magic-link?t=1".into())
            );
        }
    }

    #[tokio::test]
    async fn a_refused_key_a_redirect_and_a_tool_error_are_named() {
        for (status, code) in [
            (403, INBOX_REFUSED),
            (401, INBOX_REFUSED),
            (302, INBOX_REFUSED),
            (500, INBOX_UNREACHABLE),
        ] {
            let client = Client::new(
                &stand_in(status, false).await,
                "client.access",
                "fixture-secret",
            )
            .unwrap();
            assert_eq!(client.mailboxes().await, Err(code.to_string()), "{status}");
        }
        // A tool error: isError in the result.
        use axum::{routing::post, Router};
        let app = Router::new().route(
            "/mcp",
            post(|| async {
                json!({"jsonrpc": "2.0", "id": 1, "result": {"isError": true, "content": [{"type": "text", "text": "forbidden mailbox"}]}}).to_string()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::new(
            &format!("http://127.0.0.1:{port}"),
            "client.access",
            "fixture-secret",
        )
        .unwrap();
        assert_eq!(
            client
                .search("cloudflare:a@b.example", "a@b.example", 0)
                .await,
            Err(INBOX_FAILED.to_string())
        );
        // Nobody listening.
        let gone = Client::new("http://127.0.0.1:9", "client.access", "fixture-secret").unwrap();
        assert_eq!(gone.mailboxes().await, Err(INBOX_UNREACHABLE.to_string()));
    }

    fn store(root: &std::path::Path) -> switchboard_core::Store {
        switchboard_core::Store::open(
            root.to_owned(),
            std::sync::Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap()
    }

    #[test]
    fn a_saved_key_keeps_its_secret_in_the_vault_and_shows_only_the_server() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        assert_eq!(status(root.path()).unwrap()["saved"], false);
        assert_eq!(
            client(root.path(), &store).err().as_deref(),
            Some(INBOX_NOT_SAVED)
        );
        let saved = set(
            root.path(),
            &store,
            "https://Inbox.Example.com/",
            " client.access ",
            "fixture-secret\n",
        )
        .unwrap();
        assert_eq!(saved["saved"], true);
        assert_eq!(saved["origin"], "https://inbox.example.com");
        let file = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        assert!(
            !file.contains("fixture-secret"),
            "the file never holds the secret"
        );
        assert!(!saved.to_string().contains("client.access"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let item = vault_item(root.path()).unwrap();
        assert_eq!(store.secret(&item).unwrap(), "fixture-secret");
        assert!(client(root.path(), &store).is_ok());
        // Saving again replaces the same vault item.
        set(
            root.path(),
            &store,
            "https://inbox.example.com",
            "id2",
            "secret-2",
        )
        .unwrap();
        assert_eq!(vault_item(root.path()).unwrap(), item);
        assert_eq!(store.secret(&item).unwrap(), "secret-2");
        // Removing deletes the vault item and the file; again, nothing is there to remove.
        assert_eq!(remove(root.path(), &store).unwrap()["removed"], true);
        assert!(store.secret(&item).is_err());
        assert_eq!(status(root.path()).unwrap()["saved"], false);
        assert_eq!(remove(root.path(), &store).unwrap()["removed"], false);
    }

    #[test]
    fn a_wrong_server_or_key_saves_nothing_and_a_hand_edited_file_is_not_used() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        let refused = |origin: &str, id: &str, secret: &str| {
            set(root.path(), &store, origin, id, secret).err()
        };
        assert_eq!(
            refused("http://inbox.example.com", "id", "secret").as_deref(),
            Some(INBOX_ORIGIN_INVALID)
        );
        assert_eq!(
            refused("https://inbox.example.com", "id", "two words").as_deref(),
            Some(INBOX_KEY_INVALID)
        );
        assert_eq!(
            refused("https://inbox.example.com", "", "secret").as_deref(),
            Some(INBOX_KEY_INVALID)
        );
        assert!(!root.path().join(FILE).exists());
        set(
            root.path(),
            &store,
            "https://inbox.example.com",
            "id",
            "secret",
        )
        .unwrap();
        let file = root.path().join(FILE);
        let edited = std::fs::read_to_string(&file)
            .unwrap()
            .replace("https://inbox.example.com", "http://evil.example");
        switchboard_core::private_fs::private_write(&file, edited.as_bytes()).unwrap();
        assert_eq!(status(root.path()).err().as_deref(), Some(INBOX_UNREADABLE));
        assert_eq!(
            client(root.path(), &store).err().as_deref(),
            Some(INBOX_UNREADABLE)
        );
        // Removing still clears an unreadable file.
        assert_eq!(remove(root.path(), &store).unwrap()["removed"], true);
    }

    #[tokio::test]
    async fn a_check_says_whether_the_server_answers_the_saved_key() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path());
        assert_eq!(check(root.path(), &store).await.unwrap()["saved"], false);
        let server = stand_in(200, false).await;
        set(
            root.path(),
            &store,
            &server,
            "client.access",
            "fixture-secret",
        )
        .unwrap();
        let view = check(root.path(), &store).await.unwrap();
        assert_eq!(
            (&view["answer"], &view["mailboxes"]),
            (&json!("ok"), &json!(1))
        );
        let server = stand_in(403, false).await;
        set(
            root.path(),
            &store,
            &server,
            "client.access",
            "fixture-secret",
        )
        .unwrap();
        let view = check(root.path(), &store).await.unwrap();
        assert_eq!(
            (&view["answer"], &view["mailboxes"]),
            (&json!("refused"), &Value::Null)
        );
    }

    #[test]
    fn a_sign_in_again_is_for_the_captured_e_mail() {
        let identity = ExternalIdentity {
            account_id: None,
            organization_id: None,
            email: Some("Person@Mail.example".into()),
        };
        assert_eq!(
            sign_in_email(Some(&identity)),
            Some("person@mail.example".into())
        );
        assert_eq!(sign_in_email(None), None);
        let none = ExternalIdentity {
            email: None,
            ..identity
        };
        assert_eq!(sign_in_email(Some(&none)), None);
    }
}
