//! The one rule for the characters of text that comes into azul.
//!
//! A document's markup (before a loader tokenizes it, and what its character references
//! decode to), the text a node is built or replaced with, the text attributes a person reads,
//! typed, composed, pasted and accessibility-set text, and font family names all pass
//! [`normalize_input_text`]. So what reads text after them (shaping, line breaking, a PDF's
//! `ToUnicode` map, the accessibility tree, the markup writer) never meets these characters
//! and need not guard against them. Each special case is a row of [`INPUT_CHAR_RULES`] that
//! cites the standard it follows; a character no row names is kept.

use alloc::{borrow::Cow, string::String};
use core::cmp::Ordering;

use crate::AzString;

/// What the rule does with the characters of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputCharAction {
    /// The character is written as this one.
    Replace(char),
    /// A carriage return: dropped before a line feed, a line feed anywhere else.
    LineBreak,
}

/// One row of [`INPUT_CHAR_RULES`]: the characters `first..=last` and what becomes of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputCharRule {
    /// The first character of the row.
    pub first: char,
    /// The last character of the row.
    pub last: char,
    /// What the rule does with them.
    pub action: InputCharAction,
}

const fn row(first: char, last: char, action: InputCharAction) -> InputCharRule {
    InputCharRule {
        first,
        last,
        action,
    }
}

/// U+FFFD REPLACEMENT CHARACTER, which stands for a character that cannot be kept.
const REPLACEMENT: InputCharAction = InputCharAction::Replace('\u{FFFD}');

/// The characters the rule changes, sorted and disjoint; every other character stays.
///
/// Kept on purpose, as both HTML and XML 1.0 keep them: tab and line feed, DEL and the C1
/// controls, and the noncharacters U+FDD0 to U+FDEF and the last two of each plane above the
/// first (U+1FFFE, U+1FFFF ... U+10FFFF), which XML counts as characters and Unicode allows
/// in interchange (Corrigendum #9).
pub const INPUT_CHAR_RULES: &[InputCharRule] = &[
    // NULL: HTML's tokenizer reads it as U+FFFD wherever its tree construction does not
    // ignore it (13.2.5, "unexpected-null-character"); XML 1.0 has no NULL (2.2, `Char`),
    // and a C string ends at one.
    row('\u{0}', '\u{0}', REPLACEMENT),
    // The C0 controls but tab, line feed, form feed and carriage return: HTML keeps them as a
    // parse error (13.2.3.5, "control-character-in-input-stream"), but they are no XML 1.0
    // `Char`, so no XML loader reads them and no XML writer can write them. U+FFFD keeps them
    // visible, as CSS Text 3 asks a control character to be painted, instead of dropping them.
    row('\u{1}', '\u{8}', REPLACEMENT),
    row('\u{B}', '\u{B}', REPLACEMENT),
    // FORM FEED: white space to HTML, no XML `Char`: the space both read alike.
    row('\u{C}', '\u{C}', InputCharAction::Replace(' ')),
    // CARRIAGE RETURN: CR LF and a lone CR are one line feed (HTML 13.2.3.5, "normalize
    // newlines"; XML 1.0 2.11, "End-of-Line Handling").
    row('\r', '\r', InputCharAction::LineBreak),
    row('\u{E}', '\u{1F}', REPLACEMENT),
    // U+FFFE and U+FFFF: noncharacters HTML keeps as a parse error (13.2.3.5,
    // "noncharacter-in-input-stream") but XML 1.0 has no `Char` for. In UTF-16 the first
    // reads as a byte-swapped byte order mark and the second as the end-of-text sentinel of
    // text iterators.
    row('\u{FFFE}', '\u{FFFF}', REPLACEMENT),
];

/// The UTF-8 lead byte of the character `code`.
const fn lead_byte(code: u32) -> usize {
    let lead = if code < 0x80 {
        code
    } else if code < 0x800 {
        0xC0 | (code >> 6)
    } else if code < 0x1_0000 {
        0xE0 | (code >> 12)
    } else {
        0xF0 | (code >> 18)
    };
    lead as usize
}

/// Which bytes start a character one of `rules` names.
///
/// The scan's filter, made from the rows: a text without such a byte is passed without a
/// single lookup.
const fn lead_bytes_of(rules: &[InputCharRule]) -> [bool; 256] {
    let mut leads = [false; 256];
    let mut i = 0;
    while i < rules.len() {
        let mut code = rules[i].first as u32;
        while code <= rules[i].last as u32 {
            leads[lead_byte(code)] = true;
            code += 1;
        }
        i += 1;
    }
    leads
}

/// The lead bytes of the characters [`INPUT_CHAR_RULES`] names.
static RULE_LEAD_BYTES: [bool; 256] = lead_bytes_of(INPUT_CHAR_RULES);

/// What the rule does with `c`; `None` when `c` stays as it is.
#[must_use]
pub fn input_char_action(c: char) -> Option<InputCharAction> {
    INPUT_CHAR_RULES
        .binary_search_by(|rule| {
            if rule.last < c {
                Ordering::Less
            } else if rule.first > c {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()
        .and_then(|i| INPUT_CHAR_RULES.get(i))
        .map(|rule| rule.action)
}

/// The byte index of the first character of `text` the rule changes.
///
/// A byte the filter names is never inside a character, so it is where one starts.
fn first_change(text: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = text
        .as_bytes()
        .get(from..)?
        .iter()
        .position(|&b| RULE_LEAD_BYTES[usize::from(b)])
    {
        let at = from + offset;
        let c = text.get(at..)?.chars().next()?;
        if input_char_action(c).is_some() {
            return Some(at);
        }
        from = at + c.len_utf8();
    }
    None
}

/// `text` with its characters as the rule has them; borrowed when none of them changes.
#[must_use]
pub fn normalize_input_text(text: &str) -> Cow<'_, str> {
    let Some(start) = first_change(text) else {
        return Cow::Borrowed(text);
    };
    let (kept, rest) = text.split_at(start);
    let mut out = String::with_capacity(text.len() + 2);
    out.push_str(kept);
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match input_char_action(c) {
            None => out.push(c),
            Some(InputCharAction::Replace(with)) => out.push(with),
            Some(InputCharAction::LineBreak) => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
        }
    }
    Cow::Owned(out)
}

/// [`normalize_input_text`] of an owned string: `text` itself when no character changes.
#[must_use]
pub fn normalize_input_string(text: String) -> String {
    if first_change(&text).is_none() {
        return text;
    }
    normalize_input_text(&text).into_owned()
}

/// `text` with its characters as the rule has them, when one of them changes.
///
/// `None` when it holds them so already. A translation key ([`AzString::tr`]) stays one.
#[must_use]
pub fn normalized_az_string(text: &AzString) -> Option<AzString> {
    match normalize_input_text(text.as_str()) {
        Cow::Borrowed(_) => None,
        Cow::Owned(normalized) => {
            let mut out = AzString::from_string(normalized);
            out.set_localizable(text.is_localizable());
            Some(out)
        }
    }
}

/// [`normalized_az_string`] of an owned string: `text` itself when no character changes.
#[must_use]
pub fn normalize_input_az_string(text: AzString) -> AzString {
    normalized_az_string(&text).unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_are_sorted_and_disjoint() {
        for rule in INPUT_CHAR_RULES {
            assert!(rule.first <= rule.last, "{rule:?}");
        }
        for pair in INPUT_CHAR_RULES.windows(2) {
            assert!(pair[0].last < pair[1].first, "{:?} {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn every_character_a_row_names_is_found_in_a_text() {
        for rule in INPUT_CHAR_RULES {
            for c in [rule.first, rule.last] {
                let text = format!("ab\u{FF01}{c}cd");
                assert!(
                    matches!(normalize_input_text(&text), Cow::Owned(_)),
                    "U+{:04X} is not found",
                    u32::from(c)
                );
            }
        }
    }

    #[test]
    fn each_row_writes_its_characters_as_the_row_says() {
        for (raw, ruled) in [
            ("a\u{0}b", "a\u{FFFD}b"),
            ("a\u{1}\u{8}b", "a\u{FFFD}\u{FFFD}b"),
            ("a\u{B}b", "a\u{FFFD}b"),
            ("a\u{C}b", "a b"),
            ("a\r\nb\rc\r\r\nd\r", "a\nb\nc\n\nd\n"),
            ("a\u{E}\u{1F}b", "a\u{FFFD}\u{FFFD}b"),
            ("a\u{FFFE}38020\u{FFFF}", "a\u{FFFD}38020\u{FFFD}"),
        ] {
            assert_eq!(normalize_input_text(raw), ruled, "{raw:?}");
            assert_eq!(normalize_input_string(raw.to_string()), ruled, "{raw:?}");
        }
    }

    #[test]
    fn a_text_the_rule_leaves_alone_is_borrowed() {
        for text in [
            "",
            "plain text",
            "tab\tand\nline",
            "caf\u{E9} \u{65E5}\u{672C}\u{8A9E} \u{1F642}",
            // Characters with the lead byte of U+FFFE: fullwidth forms, specials.
            "\u{FF01}\u{FF21}\u{FFE0}\u{FFFC}\u{FFFD}",
            // DEL, the C1 controls and the noncharacters XML can carry stay.
            "\u{7F}\u{85}\u{9F}\u{FDD0}\u{FDEF}\u{1FFFE}\u{1FFFF}\u{10FFFE}\u{10FFFF}",
        ] {
            assert!(
                matches!(normalize_input_text(text), Cow::Borrowed(_)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn reading_a_text_twice_changes_nothing_more() {
        let raw = "a\u{0}b\r\n\r\nc\rd\u{C}e\u{FFFF}\u{FFFE}\u{1F}\u{1FFFE}";
        let once = normalize_input_text(raw).into_owned();
        assert_eq!(normalize_input_text(&once), once.as_str());
        assert!(matches!(normalize_input_text(&once), Cow::Borrowed(_)));
    }

    #[test]
    fn a_translation_key_stays_one() {
        let key = AzString::tr("greeting\u{0}");
        let ruled = normalize_input_az_string(key);
        assert_eq!(ruled.as_str(), "greeting\u{FFFD}");
        assert!(ruled.is_localizable());
        assert!(normalized_az_string(&AzString::from("plain")).is_none());
    }
}
