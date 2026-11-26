//! The vocabulary of a session: the words an app wants written as it spells them (names,
//! identifiers, jargon). One reading of it for both recognizers: what each is handed
//! ([`Vocabulary::joined`], [`Vocabulary::first`]), which words of an answer it favoured
//! ([`Vocabulary::mark`]: favouring pulls the probability of a RIGHT word down, a correct "enum"
//! came out at 0.32), and whether an entry is an identifier ([`looks_like_identifier`]:
//! `CssError`, `snake_case`): the system's recognizer writes those as spelt when it is given them
//! (`CssError` in 3 of 3 TTS voices), Whistle does not (0 of 3), which decides the order of the
//! chain ([`super::order`]). std only.
//!
//! # Evidence (TTS voices Samantha, Daniel, Fred; 19 clips; files only; 2026-11-26)
//!
//! How 33 identifiers (`CssError`, `SpeechSession`, `AzReview`, `DataTableCell`, `unwrap_or_else`,
//! `ScrollManager`, `HttpRequest`, spoken in 9 clips by 3 voices) were written, as: identifier as
//! written / spoken parts ("css error") / letters ("C. S. S.") / something else:
//!
//! | vocabulary given | system's recognizer | built-in model (Whistle) |
//! |------------------|---------------------|--------------------------|
//! | none | 0 / 11 / 0 / 22 | 0 / 12 / 3 / 18 |
//! | the compounds | **15** / 3 / 0 / 15 | 4 / 10 / 3 / 16 |
//! | their spoken parts | 0 / **19** / 0 / 14 | 0 / 16 / 3 / 14 |
//! | both | 9 / 10 / 0 / 14 | 1 / 15 / 3 / 14 |
//! | acronyms in capitals | 0 / 11 / 0 / 22 | 0 / 14 / 1 / 18 |
//! | keywords (`enum`, `await`) | 0 / 12 / 0 / 21 | 2 / 14 / 3 / 14 |
//! | compounds, parts, acronyms and keywords | 9 / 10 / 0 / 14 | 3 / **18** / 1 / **11** |
//!
//! The 27 keyword words (`enum struct await async mutex JSON HTML DOM`) written right: system 7
//! without a vocabulary, 9 with the whole one; built-in 12 without, 18 with it. Ordinary speech
//! ("hello world this is a test", "see us error", "the enemy is in the room"; 18 answers per row):
//! no code word came out of any vocabulary, in either recognizer. So the system's recognizer is
//! given the entries as the app spelt them (it writes the compound), and the built-in model the
//! entries and, after each identifier, its spoken parts ([`Vocabulary::with_spoken_parts`]). The
//! script that made the table is `vocab-matrix/run.py` (kept with the speech research notes), its
//! raw answers and the table of every word's probability are `table.md` and `summary.md` beside it.

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

/// The words an identifier is said as, lower case with single spaces: `CssError` is "css error",
/// `unwrap_or_else` "unwrap or else", `HTTPRequest` "http request", `getHTTPResponseCode` "get http
/// response code", `x86_64` "x86 64" (a digit stays with the letters before it, a capital after a
/// digit begins a word). `None` for what is not an identifier ([`looks_like_identifier`]), for a
/// phrase, and for one that has a single word.
///
/// The built-in model cannot write a compound but writes its words right when it is given them
/// (see the evidence in the module's table); the system's recognizer is given the compound alone.
#[must_use]
pub fn spoken_parts(entry: &str) -> Option<String> {
    let entry = entry.trim();
    if entry.is_empty() || entry.split_whitespace().count() != 1 || !looks_like_identifier(entry) {
        return None;
    }
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = entry.chars().collect();
    for (at, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            // An underscore (or any other mark) ends the word.
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        let begins = at > 0
            && !current.is_empty()
            && c.is_uppercase()
            && (chars[at - 1].is_lowercase()
                || chars[at - 1].is_ascii_digit()
                || (chars[at - 1].is_uppercase() && chars.get(at + 1).is_some_and(|next| next.is_lowercase())));
        if begins {
            words.push(std::mem::take(&mut current));
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    (words.len() > 1).then(|| words.join(" "))
}

impl Vocabulary {
    /// This vocabulary with, after each identifier in it, its spoken parts ([`spoken_parts`]; not
    /// added where the vocabulary has them already), kept to `max_entries` (what is really sent
    /// counts), and whether some were left out. For the built-in model; the system's recognizer
    /// is given the entries as they are.
    #[must_use]
    pub fn with_spoken_parts(&self, max_entries: usize) -> (Self, bool) {
        let has = |list: &[String], text: &str| list.iter().any(|e| e.eq_ignore_ascii_case(text));
        let mut sent: Vec<String> = Vec::new();
        let mut left_out = false;
        for entry in &self.entries {
            if has(&sent, entry) {
                continue;
            }
            if sent.len() >= max_entries {
                left_out = true;
                break;
            }
            sent.push(entry.clone());
            if let Some(parts) = spoken_parts(entry) {
                if !has(&self.entries, &parts) && !has(&sent, &parts) {
                    if sent.len() >= max_entries {
                        left_out = true;
                        break;
                    }
                    sent.push(parts);
                }
            }
        }
        (Self::of(sent), left_out)
    }

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
