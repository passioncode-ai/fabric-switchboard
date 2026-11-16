//! The core's operations events (SB-397 W1.3, D-2): a module writes an event here and the app
//! that runs the core sends it on to its own log. Switchboard installs its `oplog` as the sink;
//! without a sink an event is dropped. An event carries codes, numbers and yes/no only, never text
//! a caller or a session wrote.
use std::sync::OnceLock;

/// One field of an event: a fixed code, a number or a yes/no. There is deliberately no string
/// variant that takes an owned or borrowed runtime value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Code(&'static str),
    Number(i64),
    Flag(bool),
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
