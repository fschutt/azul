//! The vocabulary of a session: the words an app wants written as it spells them (names,
//! identifiers, jargon). One reading of it for both recognizers: what each is handed
//! ([`Vocabulary::joined`], [`Vocabulary::first`]), which words of an answer it favoured
//! ([`Vocabulary::mark`]: favouring pulls the probability of a RIGHT word down, a correct "enum"
//! came out at 0.32), and whether an entry is an identifier ([`looks_like_identifier`]:
//! `CssError`, `snake_case`): the system's recognizer writes those as spelt when it is given them
//! (`CssError` in 3 of 3 TTS voices), Whistle does not (0 of 3), which decides the order of the
//! chain ([`super::order`]). std only.

use std::collections::HashSet;

use super::recognizer::Word;

/// The words a recognizer is asked to favour, kept to limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vocabulary {
    /// The entries as the app spelt them, cleaned.
    entries: Vec<String>,
    /// The words of the entries, compared by [`key`].
    keys: HashSet<String>,
}

/// A word as it is compared: lower case, letters, digits and underscores only, so "Enum," is
/// "enum" and "unwrap_or_else" stays one word.
#[must_use]
pub fn key(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether an entry is spelt like an identifier: an underscore between letters or digits
/// (`snake_case`), a capital inside it (`CssError`, `cssError`, `iPhone`), or an acronym that runs
/// into a word (`HTTPRequest`). `CSS`, `JSON` and `Rust` are not.
#[must_use]
pub fn looks_like_identifier(entry: &str) -> bool {
    let chars: Vec<char> = entry.chars().collect();
    if chars.contains(&'_') && chars.iter().any(|c| c.is_alphanumeric()) {
        return true;
    }
    for at in 1..chars.len() {
        let (before, here) = (chars[at - 1], chars[at]);
        if !here.is_uppercase() {
            continue;
        }
        if before.is_lowercase() {
            return true;
        }
        if before.is_uppercase() && at >= 2 && chars.get(at + 1).is_some_and(|next| next.is_lowercase()) {
            return true;
        }
    }
    false
}

impl Vocabulary {
    /// `words` kept to `max_entries` entries of at most `max_chars` characters (control
    /// characters dropped, the ends trimmed, empty ones skipped), and the sentence about what was
    /// cut (`None` when nothing was).
    #[must_use]
    pub fn from_words(words: &[String], max_entries: usize, max_chars: usize) -> (Self, Option<String>) {
        let mut entries = Vec::new();
        let mut cut = false;
        for word in words {
            let clean: String = word.chars().filter(|c| !c.is_control()).collect();
            let clean = clean.trim();
            if clean.is_empty() {
                continue;
            }
            if entries.len() >= max_entries {
                cut = true;
                break;
            }
            let clean: String = if clean.chars().count() > max_chars {
                cut = true;
                clean.chars().take(max_chars).collect()
            } else {
                clean.to_string()
            };
            entries.push(clean);
        }
        let note = cut.then(|| format!("the vocabulary was cut to {max_entries} words of at most {max_chars} characters"));
        (Self::of(entries), note)
    }

    fn of(entries: Vec<String>) -> Self {
        let keys = entries
            .iter()
            .flat_map(|entry| entry.split_whitespace())
            .map(key)
            .filter(|k| !k.is_empty())
            .collect();
        Self { entries, keys }
    }

    /// The first `count` entries (for a recognizer that takes fewer), and whether some were left
    /// out.
    #[must_use]
    pub fn first(&self, count: usize) -> (Self, bool) {
        if self.entries.len() <= count {
            return (self.clone(), false);
        }
        (Self::of(self.entries[..count].to_vec()), true)
    }

    /// The entries, as the app spelt them.
    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// One entry a line, for an engine that takes its keywords as one text.
    #[must_use]
    pub fn joined(&self) -> String {
        self.entries.join("\n")
    }

    /// Whether `word` is a word of the vocabulary: its case and its punctuation do not matter,
    /// and a phrase of the vocabulary holds each of its words.
    #[must_use]
    pub fn marks(&self, word: &str) -> bool {
        let k = key(word);
        !k.is_empty() && self.keys.contains(&k)
    }

    /// Marks the words of an answer that are words of the vocabulary ([`Self::marks`]).
    pub fn mark(&self, words: &mut [Word]) {
        for word in words {
            word.biased |= self.marks(&word.text);
        }
    }

    /// Whether an entry is spelt like an identifier ([`looks_like_identifier`]; of a phrase, one
    /// of its words).
    #[must_use]
    pub fn has_identifier(&self) -> bool {
        self.entries
            .iter()
            .flat_map(|entry| entry.split_whitespace())
            .any(looks_like_identifier)
    }
}

#[cfg(test)]
#[path = "vocabulary_test.rs"]
mod vocabulary_test;
