//! SB-256 N2: whether Fabric Inbox receives the e-mail a Claude sign-in is for. The sign-in
//! autopilot (docs/research/signin-autopilot-2026-11-05.md) runs only for such an address;
//! every other one keeps the ordinary Terminal sign-in. The mailboxes come from Fabric Inbox's
//! account list (`list_accounts`, `GET /api/inbox`: each account's `email` and, for a Cloudflare
//! address that keeps the mail of every other address on its domain, `catchAll: true`), read by
//! the Inbox client of node N3; here they are data only, never an instruction.
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
