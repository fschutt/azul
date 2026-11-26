//! The words of the page being reviewed, for the speech recogniser's keyword biasing and for the agent's request:
//! the identifiers, the keywords of the language and the file's names, which a recogniser trained on speech does not
//! know (`enum`, `await` and a project's own names came out right only with them).
//!
//! Pure: the page's lines and the file's path in, a capped list out (most used first, then in the order met), the
//! same list for the same page.

use std::collections::HashMap;

/// The shortest word kept: shorter ones are noise (`x`, `i`, `ab`).
pub const MIN_WORD_CHARS: usize = 3;
/// The longest: a longer one is a literal or a hash, not something said.
pub const MAX_WORD_CHARS: usize = 40;

/// Whether `c` belongs to an identifier.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `word` is worth biasing for: long enough, not too long, and not only digits and underscores.
fn is_kept(word: &str) -> bool {
    let chars = word.chars().count();
    (MIN_WORD_CHARS..=MAX_WORD_CHARS).contains(&chars) && word.chars().any(char::is_alphabetic)
}

/// The words of `lines` and of the folders and file name in `file`, at most `cap`: the words used most first, the rest in
/// the order they are met.
#[must_use]
pub fn vocabulary(lines: &[String], file: &str, cap: usize) -> Vec<String> {
    if cap == 0 {
        return Vec::new();
    }
    // The page's words with how often they occur and where they first occurred.
    let mut seen: HashMap<String, (usize, usize)> = HashMap::new();
    let mut order = 0usize;
    let mut take = |text: &str, weight: usize| {
        for word in text.split(|c: char| !is_word_char(c)).filter(|w| is_kept(w)) {
            let entry = seen.entry(word.to_string()).or_insert_with(|| {
                order += 1;
                (0, order)
            });
            entry.0 += weight;
        }
    };
    for line in lines {
        take(line, 1);
    }
    // The file name and its folders: said when the person names the file; the extension is not (`rs`).
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    take(stem, 1);
    let mut words: Vec<(&String, &(usize, usize))> = seen.iter().collect();
    words.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.1 .1.cmp(&b.1 .1)));
    words.into_iter().take(cap).map(|(w, _)| w.clone()).collect()
}

#[cfg(test)]
#[path = "vocabulary_test.rs"]
mod vocabulary_test;
