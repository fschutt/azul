//! Search boxes: "every word of the query, in any case, diacritics folded". AzMail's message
//! list, AzTasks' search and AzContacts' list each had a copy, and only AzContacts' found
//! "Krüger" for "kruger" (scripts/DEDUP_EDITORS_2026_10_02.md, B16).
//!
//! A [`Query`] is the search box's words, folded ([`fold`]); a word typed as a tag (`#home`)
//! loses its `#`. It matches a text when every word is in it. An empty query matches every text:
//! an app that shows nothing for an empty search says so itself.

/// Lower case without the common Latin diacritics: `Krüger` -> `kruger`, `Łukasz` -> `lukasz`,
/// `Straße` -> `strasse`, `Æsir` -> `aesir`; other scripts as they are.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let mapped: &str = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'æ' => "ae",
            'ç' | 'ć' | 'č' | 'ĉ' | 'ċ' => "c",
            'ď' | 'đ' | 'ð' => "d",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
            'ğ' | 'ģ' => "g",
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => "i",
            'ķ' => "k",
            'ł' | 'ľ' | 'ļ' | 'ĺ' => "l",
            'ñ' | 'ń' | 'ň' | 'ņ' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
            'œ' => "oe",
            'ŕ' | 'ř' => "r",
            'ś' | 'š' | 'ş' | 'ș' => "s",
            'ß' => "ss",
            'ť' | 'ţ' | 'ț' => "t",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
            'ý' | 'ÿ' => "y",
            'ź' | 'ż' | 'ž' => "z",
            'þ' => "th",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(mapped);
    }
    out
}

/// What a search box holds: its words, folded, each without a leading `#`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    words: Vec<String>,
}

impl Query {
    /// The words of `text` (split at blanks), folded; a `#` before a word goes, a word that is
    /// only `#` is none.
    #[must_use]
    pub fn parse(text: &str) -> Query {
        Query {
            words: text
                .split_whitespace()
                .map(|w| fold(w.trim_start_matches('#')))
                .filter(|w| !w.is_empty())
                .collect(),
        }
    }

    /// The folded words.
    #[must_use]
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// No words: the box is empty (or only blanks and `#`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Whether every word is in `text` (folded here). An empty query matches every text.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        self.is_empty() || self.matches_folded(&fold(text))
    }

    /// [`Query::matches`] for a text already folded (a list folds its rows once, then asks for
    /// every keystroke).
    #[must_use]
    pub fn matches_folded(&self, folded: &str) -> bool {
        self.words.iter().all(|w| folded.contains(w.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diacritics_and_case_fold_away() {
        // AzContacts' book.rs test.
        assert_eq!(fold("Krüger"), "kruger");
        assert_eq!(fold("\u{141}ukasz"), "lukasz");
        assert_eq!(fold("Ångström"), "angstrom");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("\u{738b}\u{82b3}"), "\u{738b}\u{82b3}");
        assert_eq!(fold("ÆSIR Þór"), "aesir thor");
    }

    #[test]
    fn every_word_must_be_there_in_any_case_and_order() {
        let q = Query::parse("  Garden PLAN ");
        assert_eq!(q.words(), ["garden", "plan"]);
        assert!(q.matches("The plan for the garden"));
        assert!(!q.matches("The garden"));
        assert!(Query::parse("kruger").matches("Ben Krüger"));
        assert!(
            Query::parse("krüg").matches("Ben Kruger"),
            "folded on both sides"
        );
        assert!(Query::parse("café").matches("CAFE run"));
    }

    #[test]
    fn a_tag_typed_with_its_hash_is_a_word_and_an_empty_query_matches_everything() {
        // AzTasks: "#errand nine" finds a task tagged errand; AzMail / AzContacts: an empty box
        // lists everything.
        let q = Query::parse("#errand nine");
        assert_eq!(q.words(), ["errand", "nine"]);
        assert!(q.matches("buy nine stamps errand"));
        for empty in ["", "   ", " # ## "] {
            let q = Query::parse(empty);
            assert!(q.is_empty(), "{empty:?}");
            assert!(q.matches("anything"));
            assert!(q.matches(""));
        }
        assert!(Query::parse("plan").matches_folded(&fold("Garden PLAN")));
    }
}
