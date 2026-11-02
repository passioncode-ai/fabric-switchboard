//! SB-189 (CPY-03): one sentence per refusal on every surface. The backend's own text stays its
//! internal vocabulary (classifiers such as `add_refusal`, tests and the log match it); what a
//! person reads, in the window, the CLI or the MCP server, is the sentence `person-errors.json`
//! gives it, which says what happened and what to do. `src/adapter.ts` imports the same file, so
//! the window cannot drift from the terminal. A text the table does not name is shown as it is.
use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

static SENTENCES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    #[derive(serde::Deserialize)]
    struct Table {
        sentences: HashMap<String, String>,
    }
    serde_json::from_str::<Table>(include_str!("person-errors.json"))
        .map(|t| t.sentences)
        .unwrap_or_default()
});

/// The sentence a person reads for the backend refusal `raw`; `raw` itself when the table has none.
pub fn for_person(raw: &str) -> Cow<'_, str> {
    match SENTENCES.get(raw) {
        Some(sentence) => Cow::Owned(sentence.clone()),
        None => Cow::Borrowed(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_read_and_a_refusal_reads_as_its_sentence() {
        assert!(SENTENCES.len() > 150, "the embedded table parsed");
        assert_eq!(
            for_person("Project not found."),
            "This group no longer exists. Refresh the list."
        );
        // A raw sentence the table does not name, and a sentence already for a person, stay.
        assert_eq!(for_person("No account with id x."), "No account with id x.");
        let sentence = "This account no longer exists. Refresh the account list.";
        assert_eq!(for_person(sentence), sentence);
    }

    #[test]
    fn every_sentence_is_final_plain_and_maps_to_itself() {
        for (raw, sentence) in SENTENCES.iter() {
            assert!(!sentence.trim().is_empty(), "{raw}");
            assert!(
                sentence.ends_with('.') || sentence.ends_with(')'),
                "a full sentence: {sentence}"
            );
            // No em dash in text for people (AGENTS.md, the operator's rule).
            assert!(!sentence.contains('\u{2014}'), "{sentence}");
            // Applying the table twice changes nothing: no person sentence is itself a raw key
            // that maps elsewhere.
            assert_eq!(for_person(sentence), *sentence, "{sentence}");
        }
    }
}
