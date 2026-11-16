//! Identifiers a session record may carry (from switchboard-runtime `launches`, SB-397 D-2).

/// A launch, session or wave id: 1 to 64 ASCII letters, digits or hyphens, so it is safe in a
/// file name and a command line.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_id_is_short_and_plain() {
        assert!(super::valid_id("a1-B2"));
        for bad in ["", "a/b", "a b", "a.b", &"x".repeat(65)] {
            assert!(!super::valid_id(bad), "{bad:?}");
        }
    }
}
