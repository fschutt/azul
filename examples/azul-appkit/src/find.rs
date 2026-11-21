//! Find and replace in plain text, as the standard Find / Replace dialog
//! asks for it (match case, whole word) - one matcher for every app with a
//! Find (AzSheets' cells, AzShow's slides and notes), so they agree on what
//! a match is.

/// How a needle matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TextMatch {
    /// Upper and lower case differ.
    pub match_case: bool,
    /// The needle must stand as a whole word (not inside a longer one).
    pub whole_word: bool,
}

/// Whether two characters are the same under `match_case`.
fn same_char(a: char, b: char, match_case: bool) -> bool {
    a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()))
}

/// A character that is part of a word (a whole-word match stops at others).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The byte ranges of the matches of `needle` in `text`, left to right, not
/// overlapping. Compared character by character, so a case change that
/// alters a character's byte length cannot shift a range.
#[must_use]
pub fn matches(text: &str, needle: &str, how: TextMatch) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let pattern: Vec<char> = needle.chars().collect();
    let mut out = Vec::new();
    if pattern.is_empty() || pattern.len() > chars.len() {
        return out;
    }
    let mut i = 0;
    while i + pattern.len() <= chars.len() {
        let end = i + pattern.len();
        let hit = (0..pattern.len()).all(|j| same_char(chars[i + j].1, pattern[j], how.match_case));
        let whole = !how.whole_word
            || ((i == 0 || !is_word_char(chars[i - 1].1))
                && (end == chars.len() || !is_word_char(chars[end].1)));
        if hit && whole {
            out.push((chars[i].0, chars.get(end).map_or(text.len(), |c| c.0)));
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Whether `text` holds `needle`.
#[must_use]
pub fn holds(text: &str, needle: &str, how: TextMatch) -> bool {
    !matches(text, needle, how).is_empty()
}

/// `text` with every match of `needle` replaced by `replacement`; `None`
/// when nothing matches.
#[must_use]
pub fn replace(text: &str, needle: &str, replacement: &str, how: TextMatch) -> Option<String> {
    let found = matches(text, needle, how);
    if found.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (a, b) in found {
        out.push_str(&text[last..a]);
        out.push_str(replacement);
        last = b;
    }
    out.push_str(&text[last..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_honours_case_and_whole_words() {
        let any = TextMatch::default();
        assert_eq!(matches("Rent and rent", "rent", any), vec![(0, 4), (9, 13)]);
        let case = TextMatch { match_case: true, ..any };
        assert_eq!(matches("Rent and rent", "rent", case), vec![(9, 13)]);
        let whole = TextMatch { whole_word: true, ..any };
        assert!(!holds("rental", "rent", whole));
        assert!(holds("the rent.", "rent", whole));
        assert!(matches("x", "", any).is_empty());
        assert_eq!(matches("Straße", "STRASSE", any), Vec::new(), "no folding beyond simple case");
    }

    #[test]
    fn replacing_keeps_the_rest_of_the_text() {
        let any = TextMatch::default();
        assert_eq!(replace("Rent and rent", "rent", "Lease", any).as_deref(), Some("Lease and Lease"));
        assert_eq!(replace("Grüße grüße", "GRÜSSE", "x", any), None);
        assert_eq!(replace("Ärger ärger", "ärger", "Ok", any).as_deref(), Some("Ok Ok"));
        assert_eq!(replace("nothing", "rent", "x", any), None);
    }
}
