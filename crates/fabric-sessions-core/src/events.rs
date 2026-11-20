//! The core's operations events (SB-397 W1.3, D-2): a module writes an event here and the app
//! that runs the core sends it on to its own log. Switchboard installs its `oplog` as the sink;
//! without a sink an event is dropped. An event carries codes, numbers and yes/no only, never text
//! a caller or a session wrote.
use std::sync::OnceLock;

/// One field of an event: a fixed code, a number, a yes/no or an opaque id. There is deliberately
/// no string variant that takes an owned or borrowed runtime value: an id passes only through
/// `Id::new`, which keeps nothing but an opaque identifier's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Code(&'static str),
    Number(i64),
    Flag(bool),
    /// SB-587: a run's correlation id (a Fabric project, workflow, handoff, request, launch or an
    /// account's opaque handle), so a start and its end can be joined in the log.
    Id(Id),
}

/// An opaque identifier, checked at construction (SB-587): 8 to 64 characters of
/// `[A-Za-z0-9._:-]`, starting with a letter or digit, holding a run of at least 8 hexadecimal
/// digits (a UUID, `wf_<hex>`, `handoff:<hex>`, `sb-<hex>`). No folder separator, `@`, space or
/// other character a path, an e-mail, a label or a provider's message needs can pass, and a word
/// without such a run (a person's or a group's label) is refused.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Id {
    bytes: [u8; Id::MAX],
    len: u8,
}

impl Id {
    pub const MAX: usize = 64;
    const MIN: usize = 8;
    const HEX_RUN: usize = 8;

    /// The id, or nothing when `value` does not have an opaque id's shape.
    pub fn new(value: &str) -> Option<Self> {
        let raw = value.as_bytes();
        if raw.len() < Self::MIN || raw.len() > Self::MAX || !raw[0].is_ascii_alphanumeric() {
            return None;
        }
        let allowed = |b: &u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-');
        if !raw.iter().all(allowed) {
            return None;
        }
        let (mut run, mut best) = (0usize, 0usize);
        for b in raw {
            run = if b.is_ascii_hexdigit() { run + 1 } else { 0 };
            best = best.max(run);
        }
        if best < Self::HEX_RUN {
            return None;
        }
        let mut bytes = [0u8; Self::MAX];
        bytes[..raw.len()].copy_from_slice(raw);
        Some(Self {
            bytes,
            len: raw.len() as u8,
        })
    }
    pub fn as_str(&self) -> &str {
        // Only ASCII bytes are ever stored.
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or("")
    }
}

impl std::fmt::Debug for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Id({})", self.as_str())
    }
}

/// The field for `value` when it has an opaque id's shape.
pub fn id(value: &str) -> Option<Field> {
    Id::new(value).map(Field::Id)
}

/// Where events go: the app's own log.
pub type Sink = fn(&'static str, &[(&'static str, Field)]);

static SINK: OnceLock<Sink> = OnceLock::new();

/// Installs the sink once; a second call keeps the first.
pub fn set_sink(sink: Sink) {
    let _ = SINK.set(sink);
}

/// Sends one event to the sink, if one is installed.
pub fn event(name: &'static str, fields: &[(&'static str, Field)]) {
    if let Some(sink) = SINK.get() {
        sink(name, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_keeps_only_an_opaque_identifiers_shape() {
        for good in [
            "60178621-7f8f-405a-b583-cb713bf013d9",
            "wf_b682b0d9e3a8e36b",
            "handoff:3b034509b3623ceb",
            "sb-d7e2b5e75a7e",
        ] {
            assert_eq!(
                Id::new(good).map(|i| i.as_str().to_owned()),
                Some(good.to_owned())
            );
        }
        for bad in [
            "",
            "inbox",
            "Sergey Personal",
            "someone@example.com",
            "/Users/person/DATA/repo",
            "C:\\Users\\x",
            "-60178621-7f8f",
            "work-group-alpha",
            "rate limit 0123456789abcdef!",
            &"a1b2c3d4e5f6".repeat(6),
        ] {
            assert!(Id::new(bad).is_none(), "{bad}");
        }
    }
    use std::sync::Mutex;

    type Seen = (&'static str, Vec<(&'static str, Field)>);
    static SEEN: Mutex<Vec<Seen>> = Mutex::new(Vec::new());
    fn record(name: &'static str, fields: &[(&'static str, Field)]) {
        SEEN.lock().unwrap().push((name, fields.to_vec()));
    }

    #[test]
    fn an_event_reaches_the_installed_sink_and_a_second_sink_is_ignored() {
        fn other(_: &'static str, _: &[(&'static str, Field)]) {
            panic!("the first sink stays");
        }
        set_sink(record);
        set_sink(other);
        event(
            "probe",
            &[("reason", Field::Code("x")), ("n", Field::Number(2))],
        );
        let seen = SEEN.lock().unwrap();
        assert!(seen.iter().any(|(n, f)| *n == "probe"
            && f == &vec![("reason", Field::Code("x")), ("n", Field::Number(2))]));
    }
}
