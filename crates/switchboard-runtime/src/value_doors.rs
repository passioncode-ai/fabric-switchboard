//! SB-560 (W11.1, spec S3.1, M3-01): the operations that answer a credential or a provider key
//! go through restricted doors, each journalled.
//!
//! A saved provider key (`AgentKeyValue`, `AnthropicKeyValue`) is answered only for a one-time
//! ticket Switchboard wrote into the session script it launched (`SWITCHBOARD_KEY_TICKET`, set for
//! the key command alone): the ticket names the key's slot, lives ten minutes and opens it once.
//! Any other process of the same user, an agent with a shell included, gets a refusal; before,
//! naming the hand-over and reading from a pipe was enough. Tickets are kept here only as their
//! SHA-256 (`key-tickets.json`, 0600). A sign-in handed back to the machine that moved it
//! (`SurrenderAccount`, `SurrenderDelegated`) and every key answer or refusal write one line to
//! `value-doors.jsonl` (0600, the newest 500): the kind, how it ended and when; never a value, a
//! ticket or a key's name.
use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub const TICKETS_FILE: &str = "key-tickets.json";
pub const JOURNAL_FILE: &str = "value-doors.jsonl";
/// The variable a launch sets for its key command alone.
pub const TICKET_ENV: &str = "SWITCHBOARD_KEY_TICKET";
/// How long a ticket opens its key.
pub const TICKET_SECONDS: i64 = 600;
const MAX_TICKETS: usize = 64;
const KEEP_LINES: usize = 500;

pub const TICKET_REQUIRED: &str = "A saved key is handed only to a session Switchboard launched, once, from its own script. Launch the agent with Switchboard, or see what is saved with switchboard agents openrouter status or switchboard agents anthropic list.";

/// The writes of this process to the two files, one at a time.
static WRITE: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ticket {
    /// SHA-256 of the ticket, hex.
    hash: String,
    /// What it opens: `openrouter:machine`, `openrouter:group:<pool>` or `anthropic:<name>`.
    slot: String,
    expires_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Tickets {
    tickets: Vec<Ticket>,
}

fn hash(ticket: &str) -> String {
    digest(&SHA256, ticket.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read(root: &Path) -> Tickets {
    switchboard_core::private_fs::read_private(&root.join(TICKETS_FILE), 64 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(root: &Path, tickets: &Tickets) -> Result<(), String> {
    let file = root.join(TICKETS_FILE);
    if tickets.tickets.is_empty() {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(TICKET_REQUIRED.into()),
            _ => Ok(()),
        };
    }
    let bytes = serde_json::to_vec(tickets).map_err(|_| TICKET_REQUIRED.to_string())?;
    switchboard_core::private_fs::private_write(&file, &bytes)
        .map_err(|_| TICKET_REQUIRED.to_string())
}

/// The slot word of a saved key of `service` (`openrouter`) in its slot.
pub fn key_slot(service: &str, slot: &crate::spend::KeySlot) -> String {
    match slot {
        crate::spend::KeySlot::Machine => format!("{service}:machine"),
        crate::spend::KeySlot::Group(pool) => format!("{service}:group:{pool}"),
    }
}

/// The slot word of a named Anthropic key.
pub fn anthropic_slot(name: &str) -> String {
    format!("anthropic:{name}")
}

/// A new ticket for `slot`, valid `TICKET_SECONDS` from `now`; only its hash is kept here.
pub fn issue(root: &Path, slot: &str, now: i64) -> Result<String, String> {
    let mut bytes = [0u8; 32];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
        .map_err(|_| TICKET_REQUIRED.to_string())?;
    let ticket: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let mut tickets = read(root);
    tickets.tickets.retain(|t| t.expires_at > now);
    tickets.tickets.push(Ticket {
        hash: hash(&ticket),
        slot: slot.to_owned(),
        expires_at: now + TICKET_SECONDS,
    });
    let over = tickets.tickets.len().saturating_sub(MAX_TICKETS);
    tickets.tickets.drain(..over);
    write(root, &tickets)?;
    Ok(ticket)
}

/// Opens `slot` once for `ticket`: the ticket is taken whatever it opens, and a missing, expired
/// or other slot's ticket is refused. Journalled under `kind`.
pub fn redeem(
    root: &Path,
    ticket: Option<&str>,
    slot: &str,
    kind: &'static str,
    now: i64,
) -> Result<(), String> {
    let opened = ticket.is_some_and(|ticket| {
        let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
        let mut tickets = read(root);
        let wanted = hash(ticket);
        let found = tickets.tickets.iter().position(|t| t.hash == wanted);
        let opens = found.is_some_and(|i| {
            let t = &tickets.tickets[i];
            t.slot == slot && t.expires_at > now
        });
        if let Some(i) = found {
            tickets.tickets.remove(i);
        }
        tickets.tickets.retain(|t| t.expires_at > now);
        // A ticket that cannot be taken off the list opens nothing.
        write(root, &tickets).is_ok() && opens
    });
    journal(root, kind, if opened { "given" } else { "refused" }, now);
    if opened {
        Ok(())
    } else {
        Err(TICKET_REQUIRED.into())
    }
}

/// One line in `value-doors.jsonl`: the kind of value, how the door ended and when.
pub fn journal(root: &Path, kind: &'static str, result: &'static str, now: i64) {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let file = root.join(JOURNAL_FILE);
    let mut lines: Vec<String> = switchboard_core::private_fs::read_private(&file, 512 * 1024)
        .ok()
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .lines()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    lines.push(serde_json::json!({"at": now, "kind": kind, "result": result}).to_string());
    let over = lines.len().saturating_sub(KEEP_LINES);
    lines.drain(..over);
    let mut text = lines.join("\n");
    text.push('\n');
    if switchboard_core::private_fs::private_write(&file, text.as_bytes()).is_err() {
        crate::oplog::event(
            "value_door",
            &[("result", crate::oplog::Field::Code("unwritten"))],
        );
    }
    crate::oplog::event(
        "value_door",
        &[
            ("kind", crate::oplog::Field::Code(kind)),
            ("result", crate::oplog::Field::Code(result)),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_opens_its_slot_once_and_only_while_it_lives() {
        let root = tempfile::tempdir().unwrap();
        let slot = key_slot("openrouter", &crate::spend::KeySlot::Machine);
        let ticket = issue(root.path(), &slot, 1_000).unwrap();
        assert_eq!(ticket.len(), 64);
        let kept = std::fs::read_to_string(root.path().join(TICKETS_FILE)).unwrap();
        assert!(!kept.contains(&ticket), "only its hash is kept");
        // Another slot's ticket opens nothing, and is spent all the same.
        assert_eq!(
            redeem(
                root.path(),
                Some(&ticket),
                &anthropic_slot("team"),
                "anthropic_key",
                1_001
            ),
            Err(TICKET_REQUIRED.into())
        );
        assert_eq!(
            redeem(root.path(), Some(&ticket), &slot, "openrouter_key", 1_002),
            Err(TICKET_REQUIRED.into())
        );
        let ticket = issue(root.path(), &slot, 1_000).unwrap();
        assert_eq!(
            redeem(root.path(), Some(&ticket), &slot, "openrouter_key", 1_003),
            Ok(())
        );
        assert_eq!(
            redeem(root.path(), Some(&ticket), &slot, "openrouter_key", 1_004),
            Err(TICKET_REQUIRED.into()),
            "once"
        );
        let late = issue(root.path(), &slot, 1_000).unwrap();
        assert_eq!(
            redeem(
                root.path(),
                Some(&late),
                &slot,
                "openrouter_key",
                1_000 + TICKET_SECONDS
            ),
            Err(TICKET_REQUIRED.into())
        );
        assert_eq!(
            redeem(root.path(), None, &slot, "openrouter_key", 1_005),
            Err(TICKET_REQUIRED.into())
        );
        let journal = std::fs::read_to_string(root.path().join(JOURNAL_FILE)).unwrap();
        let lines: Vec<&str> = journal.lines().collect();
        assert_eq!(lines.len(), 6);
        assert_eq!(lines.iter().filter(|l| l.contains("\"given\"")).count(), 1);
        assert!(!journal.contains(&ticket) && !journal.contains("team"));
        #[cfg(unix)]
        for file in [JOURNAL_FILE] {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(file))
                .unwrap()
                .permissions();
            assert_eq!(mode.mode() & 0o777, 0o600, "{file}");
        }
    }

    #[test]
    fn the_journal_keeps_the_newest_lines() {
        let root = tempfile::tempdir().unwrap();
        for n in 0..(KEEP_LINES + 3) {
            journal(root.path(), "surrender", "given", n as i64);
        }
        let journal = std::fs::read_to_string(root.path().join(JOURNAL_FILE)).unwrap();
        assert_eq!(journal.lines().count(), KEEP_LINES);
        assert!(journal.lines().next().unwrap().contains("\"at\":3"));
    }
}
