//! SB-256 N2: whether Fabric Inbox receives the e-mail a Claude sign-in is for. The sign-in
//! autopilot (docs/research/signin-autopilot-2026-11-05.md) runs only for such an address;
//! every other one keeps the ordinary Terminal sign-in. The mailboxes come from Fabric Inbox's
//! account list (`list_accounts`, `GET /api/inbox`: each account's `email` and, for a Cloudflare
//! address that keeps the mail of every other address on its domain, `catchAll: true`), read by
//! the Inbox client of node N3; here they are data only, never an instruction. N3's parsing is
//! here too: which message in a mailbox search is Anthropic's sign-in mail, and its one link.
#![cfg_attr(not(test), allow(dead_code))]

use switchboard_core::ExternalIdentity;

/// One mailbox Fabric Inbox reads, as its account list names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mailbox {
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
    /// elsewhere takes that address first is not in the account list; the mail search (N3) finds
    /// out, and finding nothing in time ends the autopilot with the ordinary sign-in.
    CatchAll,
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

/// The mailboxes in Fabric Inbox's account list (`{"accounts": [{"email", "catchAll"}, ...]}`).
/// An entry without a usable e-mail is skipped; a list that is not one gives none.
pub(crate) fn mailboxes(answer: &serde_json::Value) -> Vec<Mailbox> {
    let Some(accounts) = answer.get("accounts").and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    accounts
        .iter()
        .filter_map(|account| {
            let email = address(account.get("email")?.as_str()?)?;
            let catch_all = account
                .get("catchAll")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(Mailbox { email, catch_all })
        })
        .collect()
}

/// Whether Fabric Inbox receives `email`: one of its mailboxes is that address, or a catch-all
/// mailbox serves its domain. None for an address it does not receive, or one that is not an
/// address.
pub(crate) fn receives(email: &str, mailboxes: &[Mailbox]) -> Option<Receives> {
    let email = address(email)?;
    if mailboxes.iter().any(|m| m.email == email) {
        return Some(Receives::Mailbox);
    }
    let domain = email.split_once('@')?.1;
    mailboxes
        .iter()
        .any(|m| m.catch_all && m.email.split_once('@').is_some_and(|(_, d)| d == domain))
        .then_some(Receives::CatchAll)
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

/// The id of the newest sign-in message in a mailbox search's answer (`{"emails": [...]}` for a
/// Cloudflare mailbox, `{"messages": [...]}` for a Gmail, IMAP or Outlook account): one to
/// `email`, from one of Anthropic's domains, that arrived at or after `since` (seconds since the
/// epoch, the moment the browser asked for the mail). A row with an unreadable date never counts.
pub(crate) fn sign_in_message(
    answer: &serde_json::Value,
    email: &str,
    since: i64,
) -> Option<String> {
    let email = address(email)?;
    let rows = answer
        .get("emails")
        .or_else(|| answer.get("messages"))?
        .as_array()?;
    rows.iter()
        .filter_map(|row| {
            let text = |key: &str| row.get(key).and_then(|v| v.as_str());
            let sender = header_address(text("sender")?)?;
            let domain = sender.split_once('@')?.1.to_owned();
            let to_us = text("recipient")?
                .split(',')
                .filter_map(header_address)
                .any(|r| r == email);
            let at = arrived(text("date")?)?;
            let id = text("id").or_else(|| text("providerMessageId"))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list() -> Vec<Mailbox> {
        // As Fabric Inbox's account list answers: a Cloudflare address, a catch-all on another
        // domain, a Gmail account and entries without a usable e-mail.
        mailboxes(&json!({
            "accounts": [
                {"id": "cloudflare:signin@one.example", "provider": "cloudflare", "email": "SignIn@One.example"},
                {"id": "cloudflare:all@two.example", "provider": "cloudflare", "email": "all@two.example", "catchAll": true},
                {"id": "gmail:7", "provider": "gmail", "email": "person@mail.example", "hidden": true},
                {"id": "gmail:8", "provider": "gmail"},
                {"id": "cloudflare:x", "provider": "cloudflare", "email": "not an address"}
            ]
        }))
    }

    #[test]
    fn the_account_list_gives_each_mailbox_with_its_catch_all() {
        assert_eq!(
            list(),
            vec![
                Mailbox {
                    email: "signin@one.example".into(),
                    catch_all: false
                },
                Mailbox {
                    email: "all@two.example".into(),
                    catch_all: true
                },
                Mailbox {
                    email: "person@mail.example".into(),
                    catch_all: false
                },
            ]
        );
        assert!(mailboxes(&json!({"accounts": "none"})).is_empty());
        assert!(mailboxes(&json!([])).is_empty());
    }

    #[test]
    fn an_address_is_received_by_its_mailbox_or_its_domains_catch_all_and_nothing_else() {
        let boxes = list();
        assert_eq!(
            receives(" SIGNIN@one.example ", &boxes),
            Some(Receives::Mailbox)
        );
        // A hidden mailbox still receives its mail.
        assert_eq!(
            receives("person@mail.example", &boxes),
            Some(Receives::Mailbox)
        );
        assert_eq!(
            receives("anyone@two.example", &boxes),
            Some(Receives::CatchAll)
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
        let answer = json!({"emails": [
            {"id": "old", "sender": "Claude <no-reply@mail.anthropic.com>", "recipient": "signin@one.example", "date": "2026-11-05T18:59:59Z"},
            {"id": "first", "sender": "Claude <no-reply@mail.anthropic.com>", "recipient": "signin@one.example", "date": "2026-11-05T19:00:10Z"},
            {"id": "newest", "sender": "no-reply@anthropic.com", "recipient": "Other <x@one.example>, SignIn@One.example", "date": "Thu, 05 Nov 2026 19:01:00 +0000"},
            {"id": "spoof", "sender": "Claude <no-reply@anthropic.com.evil.example>", "recipient": "signin@one.example", "date": "2026-11-05T19:05:00Z"},
            {"id": "elsewhere", "sender": "no-reply@anthropic.com", "recipient": "other@one.example", "date": "2026-11-05T19:06:00Z"},
            {"id": "undated", "sender": "no-reply@anthropic.com", "recipient": "signin@one.example", "date": "soon"}
        ]});
        assert_eq!(
            sign_in_message(&answer, "signin@one.example", since),
            Some("newest".into())
        );
        // A Gmail, IMAP or Outlook account's rows come as messages.
        let rows = json!({"messages": [
            {"providerMessageId": "g1", "sender": "Claude <no-reply@claude.com>", "recipient": "person@mail.example", "date": "2026-11-05T19:00:00Z"}
        ]});
        assert_eq!(
            sign_in_message(&rows, "person@mail.example", since),
            Some("g1".into())
        );
        assert_eq!(sign_in_message(&answer, "nobody@one.example", since), None);
        assert_eq!(
            sign_in_message(&json!({"emails": "x"}), "signin@one.example", since),
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
