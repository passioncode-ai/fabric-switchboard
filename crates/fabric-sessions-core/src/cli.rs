//! Talking to a command line (from switchboard-runtime `account_send`, SB-397 D-2).
use serde_json::Value;

/// `word` as one POSIX shell word: single-quoted, each `'` written as `'\\''`.
pub fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// The `data` of a JSON answer (`{"ok":true,"data":...}`) on the output's last line; None when the
/// last line is not one or says `ok: false`.
pub fn data(output: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(output.lines().last()?.trim()).ok()?;
    (value["ok"] == true).then(|| value["data"].clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_word_is_quoted_once_and_an_answer_read_from_its_last_line() {
        assert_eq!(super::quoted("it's"), "'it'\\''s'");
        assert_eq!(
            super::data("noise\n{\"ok\":true,\"data\":{\"a\":1}}"),
            Some(serde_json::json!({"a": 1}))
        );
        assert_eq!(super::data("{\"ok\":false,\"error\":\"x\"}"), None);
        assert_eq!(super::data(""), None);
    }
}
