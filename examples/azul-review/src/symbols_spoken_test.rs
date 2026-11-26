//! Spoken code: how what a recogniser wrote for "C S S error" becomes the words of `CssError`:
//! letter names into acronyms, number words into digits, the mishearings of code words, and which
//! words are only English. No window, no azul.

use super::*;
use crate::symbols::Kind;

fn heard(text: &str) -> Vec<HeardWord> {
    text.split_whitespace()
        .map(|w| HeardWord { text: w.to_string(), probability: 0.9, biased: false })
        .collect()
}

fn spoken(text: &str) -> Vec<SpokenWord> {
    clean_words(&heard(text))
}

/// The words of a workspace that has `css`, `error`, `request`, `string`, `u`, `i`, `parser`.
fn known(token: &str) -> bool {
    matches!(token, "css" | "error" | "request" | "string" | "u" | "i" | "parser" | "sha" | "utf" | "x" | "f")
}

fn none(_: &str) -> bool {
    false
}

fn tokens(found: &[Reading]) -> Vec<Vec<String>> {
    found.iter().map(|r| r.tokens.clone()).collect()
}

fn has(found: &[Reading], want: &[&str]) -> bool {
    found.iter().any(|r| r.tokens == want)
}

fn via_of(found: &[Reading], want: &[&str]) -> u8 {
    found.iter().find(|r| r.tokens == want).map_or(0, |r| r.via)
}

// ---- cleaning ----

#[test]
fn words_are_lower_case_without_the_punctuation_around_them_and_keep_their_own_text() {
    let w = spoken("Yeah, we had a (CssError), here...");
    let clean: Vec<&str> = w.iter().map(|w| w.clean.as_str()).collect();
    assert_eq!(clean, ["yeah", "we", "had", "a", "csserror", "here"]);
    assert_eq!(w[4].text, "(CssError),");
    assert!(w.iter().enumerate().all(|(i, w)| w.orig == i));
    assert_eq!(spoken("... , ?").len(), 0);
}

#[test]
fn a_spelled_out_word_with_hyphens_or_dots_is_one_word_per_letter_of_the_same_heard_word() {
    let w = spoken("the C-S-S, error and C.S.S. too");
    let clean: Vec<&str> = w.iter().map(|w| w.clean.as_str()).collect();
    assert_eq!(clean, ["the", "c", "s", "s", "error", "and", "c", "s", "s", "too"]);
    assert_eq!(w[1].orig, 1);
    assert_eq!(w[3].orig, 1);
    assert_eq!(w[4].orig, 2);
    assert_eq!(w[6].orig, 4);
    // Words with hyphens that are not letters stay whole.
    assert_eq!(spoken("well-known")[0].clean, "well-known");
}

#[test]
fn the_probability_and_the_biased_flag_travel_and_an_unknown_probability_is_none() {
    let mut h = heard("a b");
    h[0].probability = 0.13;
    h[0].biased = true;
    h[1].probability = 0.0;
    let w = clean_words(&h);
    assert_eq!(w[0].probability, Some(0.13));
    assert!(w[0].biased);
    assert_eq!(w[1].probability, None);
    h[1].probability = f32::NAN;
    assert_eq!(clean_words(&h)[1].probability, None);
    h[1].probability = 7.0;
    assert_eq!(clean_words(&h)[1].probability, Some(1.0));
}

#[test]
fn a_word_the_recogniser_wrote_as_an_identifier_or_an_acronym_says_so() {
    let w = spoken("CssError css_error iPhone CSS error Yeah utf8 HTTP2 a");
    let id: Vec<bool> = w.iter().map(|w| w.identifier_like).collect();
    let caps: Vec<bool> = w.iter().map(|w| w.allcaps).collect();
    assert_eq!(id, [true, true, true, false, false, false, true, false, false]);
    assert_eq!(caps, [false, false, false, true, false, false, false, true, false]);
}

// ---- letters ----

#[test]
fn letter_names_and_their_doubled_plurals_give_letters() {
    for (word, want) in [
        ("see", "c"),
        ("sea", "c"),
        ("esses", "ss"),
        ("ess", "s"),
        ("eye", "i"),
        ("you", "u"),
        ("s", "s"),
        ("aitch", "h"),
        ("age", "h"),
        ("zed", "z"),
        ("why", "y"),
        ("queue", "q"),
        ("tee", "t"),
        ("pee", "p"),
        ("jay", "j"),
        ("oh", "o"),
    ] {
        assert_eq!(letter_options(word), [want], "{word}");
    }
    assert!(letter_options("error").is_empty());
    assert!(letter_options("us").is_empty());
    assert!(letter_options("").is_empty());
}

#[test]
fn the_users_case_see_esses_error_is_css_error_among_its_readings() {
    for phrase in ["see esses error", "c s s error", "sea ess ess error", "see ess ess error", "C-S-S error"] {
        let w = spoken(phrase);
        let r = readings(&w, &known);
        assert!(has(&r, &["css", "error"]), "{phrase}: {:?}", tokens(&r));
        assert_ne!(via_of(&r, &["css", "error"]) & VIA_LETTERS, 0, "{phrase}");
    }
    // The literal words stay a reading too.
    let r = readings(&spoken("see esses error"), &known);
    assert!(has(&r, &["see", "esses", "error"]));
}

#[test]
fn a_letter_run_is_an_acronym_only_when_the_word_is_known_or_a_listed_acronym() {
    let w = spoken("aitch tee tee pee request");
    assert!(has(&readings(&w, &none), &["http", "request"]));
    assert!(has(&readings(&spoken("you eye"), &none), &["ui"]));
    assert!(has(&readings(&spoken("a p i"), &none), &["api"]));
    assert!(has(&readings(&spoken("jay ess oh en"), &none), &["json"]));
    // Not an acronym and not a word of the workspace: only the literal words.
    assert!(!tokens(&readings(&spoken("are you"), &none)).iter().any(|t| t == &["ru"]));
    assert!(!tokens(&readings(&spoken("see us"), &known)).iter().any(|t| t.len() == 1));
    // A short run needs the list, not only the workspace's words.
    assert!(!has(&readings(&spoken("you are"), &|t| t == "ur"), &["ur"]));
    // A longer one may be a word of the workspace.
    assert!(has(&readings(&spoken("sea ess ess"), &known), &["css"]));
    assert!(has(&readings(&spoken("sea ess ess"), &none), &["css"]), "css is a listed acronym");
}

#[test]
fn double_you_is_one_letter() {
    assert!(has(&readings(&spoken("double you ess ess"), &|t| t == "wss"), &["wss"]));
    assert!(has(&readings(&spoken("double u ess ess"), &|t| t == "wss"), &["wss"]));
}

#[test]
fn a_letter_then_a_number_is_the_name_of_a_type_or_a_format() {
    assert!(has(&readings(&spoken("u eight"), &known), &["u", "8"]));
    assert!(has(&readings(&spoken("i thirty two"), &known), &["i", "32"]));
    assert!(has(&readings(&spoken("f sixty four"), &known), &["f", "64"]));
    assert!(has(&readings(&spoken("x eighty six"), &known), &["x", "86"]));
}

// ---- numbers ----

#[test]
fn number_words_are_digits_in_the_way_a_person_says_them() {
    let n = |words: &[&str]| number_string(words);
    assert_eq!(n(&["eight"]), Some(("8".to_string(), 1)));
    assert_eq!(n(&["thirty", "two"]), Some(("32".to_string(), 2)));
    assert_eq!(n(&["two", "fifty", "six"]), Some(("256".to_string(), 3)));
    assert_eq!(n(&["sixteen"]), Some(("16".to_string(), 1)));
    assert_eq!(n(&["sixty", "four", "bit"]), Some(("64".to_string(), 2)));
    assert_eq!(n(&["one", "hundred", "twenty"]), Some(("120".to_string(), 3)));
    assert_eq!(n(&["zero"]), Some(("0".to_string(), 1)));
    assert_eq!(n(&["oh"]), None);
    assert_eq!(n(&["eight", "bit"]), Some(("8".to_string(), 1)));
    assert_eq!(n(&[]), None);
    assert_eq!(n(&["utf", "eight"]), None);
}

#[test]
fn a_number_in_words_is_a_reading_next_to_the_words_themselves() {
    let r = readings(&spoken("utf eight string"), &known);
    assert!(has(&r, &["utf", "8", "string"]), "{:?}", tokens(&r));
    assert!(has(&r, &["utf", "eight", "string"]));
    assert_ne!(via_of(&r, &["utf", "8", "string"]) & VIA_DIGITS, 0);
    assert!(has(&readings(&spoken("sha two fifty six"), &known), &["sha", "256"]));
}

// ---- mishearings ----

#[test]
fn the_mishearings_of_code_words_are_readings_beside_what_was_written() {
    let cases: [(&str, &[&str]); 12] = [
        ("enemy", &["enum"]),
        ("awake", &["await"]),
        ("a sink", &["async"]),
        ("mew tex", &["mutex"]),
        ("mute ex", &["mutex"]),
        ("jason", &["json"]),
        ("hdp", &["http"]),
        ("age ttp", &["http"]),
        ("sequel", &["sql"]),
        ("yammel", &["yaml"]),
        ("on wrap", &["unwrap"]),
        ("standard out", &["stdout"]),
    ];
    for (heard, want) in cases {
        let r = readings(&spoken(heard), &none);
        assert!(has(&r, want), "{heard}: {:?}", tokens(&r));
        assert_ne!(via_of(&r, want) & VIA_HOMOPHONE, 0, "{heard}");
    }
    // What was written is still a reading: "enemy" may be a type of a game.
    assert!(has(&readings(&spoken("enemy"), &none), &["enemy"]));
}

#[test]
fn the_mishearings_the_speech_agent_measured_are_readings() {
    for (heard, want) in [
        ("hdp request", vec!["http", "request"]),
        ("jason parser", vec!["json", "parser"]),
        ("age ttp request", vec!["http", "request"]),
        ("enemy", vec!["enum"]),
        ("awake", vec!["await"]),
    ] {
        assert!(has(&readings(&spoken(heard), &none), &want), "{heard}");
    }
}

#[test]
fn an_identifier_the_recogniser_wrote_is_split_into_its_words() {
    let r = readings(&spoken("CssError"), &none);
    assert!(has(&r, &["css", "error"]));
    assert_ne!(via_of(&r, &["css", "error"]) & VIA_IDENTIFIER, 0);
    assert!(has(&readings(&spoken("utf8"), &none), &["utf", "8"]));
    assert!(has(&readings(&spoken("css_error"), &none), &["css", "error"]));
    assert_eq!(tokens(&readings(&spoken("Error"), &none)), vec![vec!["error".to_string()]]);
}

// ---- bounds ----

#[test]
fn readings_are_bounded_whatever_the_window_and_an_empty_window_has_none() {
    assert!(readings(&[], &known).is_empty());
    let long = spoken(&"see ess ess a eye you are tee ".repeat(8));
    let started = std::time::Instant::now();
    let r = readings(&long, &known);
    assert!(r.len() <= MAX_READINGS);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    let hostile = spoken(&"one two three four five six seven eight nine ten ".repeat(10));
    assert!(readings(&hostile, &known).len() <= MAX_READINGS);
}

// ---- word lists ----

#[test]
fn stop_words_and_common_words_are_told_in_english_and_german_and_a_name_is_neither() {
    for w in ["the", "a", "we", "yeah", "und", "wir", "ich", "ist", "had", "here"] {
        assert!(is_stop_word(w), "{w}");
        assert!(is_common_word(w), "{w}");
    }
    for w in ["error", "value", "name", "data", "file", "hello", "world", "test", "message", "user"] {
        assert!(is_common_word(w), "{w}");
        assert!(!is_stop_word(w), "{w}");
    }
    for w in ["css", "scroll", "speech", "enum", "serde", "dom", "mutex", "azul", "wasm", ""] {
        assert!(!is_common_word(w), "{w:?}");
    }
    // An everyday word with an ending is an everyday word.
    for w in ["errors", "files", "wanted", "looking", "sees", "named", "users"] {
        assert!(is_common_word(w), "{w}");
    }
    for w in ["doms", "sessions", "scrolled"] {
        assert!(!is_common_word(w), "{w}");
    }
}

#[test]
fn the_word_lists_are_sorted_lower_case_and_free_of_repeats() {
    fn check(name: &str, list: &[&str]) {
        for pair in list.windows(2) {
            assert!(pair[0] < pair[1], "{name}: {} before {}", pair[0], pair[1]);
        }
        for w in list {
            assert_eq!(*w, w.to_lowercase(), "{name}: {w}");
            assert!(!w.is_empty() && !w.contains(' '), "{name}: {w:?}");
        }
    }
    check("stop", &STOP_WORDS);
    check("common", &COMMON_WORDS);
    check("acronyms", &KNOWN_ACRONYMS);
    for w in STOP_WORDS {
        assert!(is_common_word(w), "every stop word is a common word: {w}");
    }
}

#[test]
fn the_homophone_and_letter_tables_are_well_formed() {
    for (heard, target) in HOMOPHONES.iter() {
        assert!(!heard.is_empty() && !target.is_empty());
        assert!(heard.iter().chain(target.iter()).all(|w| *w == w.to_lowercase() && !w.is_empty()));
        assert_ne!(heard, target);
    }
    for (spoken, letters) in LETTER_NAMES.iter() {
        assert_eq!(*spoken, spoken.to_lowercase());
        assert!(letters.chars().all(|c| c.is_ascii_lowercase()));
    }
}

#[test]
fn a_kind_word_names_the_kinds_it_may_mean() {
    assert_eq!(kind_words("struct"), [Kind::Struct]);
    assert_eq!(kind_words("structure"), [Kind::Struct]);
    assert_eq!(kind_words("enum"), [Kind::Enum]);
    assert_eq!(kind_words("enemy"), [Kind::Enum]);
    assert_eq!(kind_words("function"), [Kind::Fn, Kind::Method]);
    assert_eq!(kind_words("method"), [Kind::Method, Kind::Fn]);
    assert_eq!(kind_words("trait"), [Kind::Trait, Kind::Interface]);
    assert_eq!(kind_words("class"), [Kind::Class, Kind::Struct]);
    assert_eq!(kind_words("module"), [Kind::Module]);
    assert_eq!(kind_words("crate"), [Kind::Package, Kind::Dependency]);
    assert_eq!(kind_words("file"), [Kind::File]);
    assert_eq!(kind_words("macro"), [Kind::Macro]);
    assert_eq!(kind_words("constant"), [Kind::Const, Kind::Static]);
    assert!(kind_words("error").is_empty());
    assert!(kind_words("").is_empty());
}

#[test]
fn the_longest_acronym_run_at_a_position_is_found_and_a_run_that_is_no_acronym_is_not() {
    let w = spoken("we use aitch tee tee pee here");
    assert_eq!(acronym_run_at(&w, 2, &none), Some((6, "http".to_string())));
    assert_eq!(acronym_run_at(&w, 3, &none), None);
    assert_eq!(acronym_run_at(&w, 0, &none), None);
    let w = spoken("c s s error");
    assert_eq!(acronym_run_at(&w, 0, &known), Some((3, "css".to_string())));
    assert_eq!(acronym_run_at(&w, 9, &known), None);
}

#[test]
fn a_plural_letter_name_is_also_the_letter_and_an_s_for_a_search_for_similar_names() {
    let r = plural_letter_readings(&spoken("sees error"));
    assert_eq!(tokens(&r), vec![vec!["cs".to_string(), "error".to_string()]]);
    assert_eq!(r[0].via, VIA_LETTERS);
    let r = plural_letter_readings(&spoken("we had esses"));
    assert_eq!(tokens(&r), vec![vec!["we".to_string(), "had".to_string(), "ss".to_string()]]);
    assert!(plural_letter_readings(&spoken("see error")).is_empty());
    assert!(plural_letter_readings(&spoken("his errors")).is_empty());
    assert!(plural_letter_readings(&[]).is_empty());
}
