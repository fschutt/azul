//! What the reviewer says, tied to the code: the sentence of the user's own ("yeah we had a CssError
//! here") in the ways a recogniser writes it, the names a person says as words, the mishearings of
//! code words, the repair of a word the recogniser doubted, and, as important, the sentences that
//! must find NOTHING. No window, no azul.

use super::*;
use crate::{
    symbols::{Caps, DefInfo, Index, Kind, RawDef, Scanned},
    symbols_spoken::{HeardWord, VIA_HOMOPHONE, VIA_IDENTIFIER, VIA_LETTERS},
};

fn put(idx: &mut Index, file: &str, defs: Vec<RawDef>) {
    idx.put_scanned(file, Scanned { size: 100, lines: 100, defs, ..Scanned::default() });
}

fn def(name: &str, kind: Kind, line: u32, lines: u32) -> RawDef {
    RawDef::new(name, kind, line).lines(lines).public()
}

/// A small workspace of crates with the names the tests speak about.
fn index() -> Index {
    let mut idx = Index::new(Caps::default());
    for dir in ["crates/core", "crates/css", "crates/layout", "crates/speech", "examples/azul-review", "examples/hello"] {
        idx.note_manifest_dir(dir);
    }
    put(&mut idx, "crates/core/Cargo.toml", vec![RawDef::new("azul-core", Kind::Package, 2)]);
    put(&mut idx, "crates/css/Cargo.toml", vec![RawDef::new("azul-css", Kind::Package, 2), RawDef::new("serde", Kind::Dependency, 9)]);
    put(&mut idx, "crates/layout/Cargo.toml", vec![RawDef::new("azul-layout", Kind::Package, 2), RawDef::new("serde", Kind::Dependency, 7)]);
    put(&mut idx, "examples/azul-review/Cargo.toml", vec![RawDef::new("AzReview", Kind::Package, 2)]);
    put(
        &mut idx,
        "crates/core/src/dom.rs",
        vec![
            def("Dom", Kind::Struct, 20, 30),
            def("Node", Kind::Struct, 60, 10),
            def("new", Kind::Method, 25, 5).parent("Dom"),
            def("len", Kind::Method, 40, 3).parent("Dom"),
        ],
    );
    put(&mut idx, "crates/core/src/ui/view.rs", vec![def("View", Kind::Struct, 5, 20), def("new", Kind::Method, 30, 4).parent("View")]);
    put(&mut idx, "crates/core/src/error.rs", vec![def("Error", Kind::Enum, 3, 8)]);
    put(&mut idx, "crates/core/src/utf.rs", vec![def("Utf8String", Kind::Struct, 4, 10), def("Sha256", Kind::Struct, 20, 5)]);
    put(&mut idx, "crates/core/src/json.rs", vec![def("JsonParser", Kind::Struct, 6, 12), def("parse", Kind::Fn, 30, 8)]);
    put(&mut idx, "crates/css/src/error.rs", vec![def("CssError", Kind::Struct, 10, 5), def("CssParseError", Kind::Enum, 30, 12)]);
    put(
        &mut idx,
        "crates/css/src/props.rs",
        vec![
            def("Color", Kind::Struct, 5, 7),
            def("Error", Kind::Enum, 40, 6),
            def("parse", Kind::Fn, 60, 20),
            def("Input", Kind::Struct, 70, 6),
            def("InputClass", Kind::Struct, 80, 6),
            def("get_margin_top", Kind::Fn, 90, 4),
            def("margin_top", Kind::Fn, 100, 4),
        ],
    );
    put(&mut idx, "crates/css/src/http.rs", vec![def("HttpRequest", Kind::Struct, 8, 9)]);
    put(&mut idx, "crates/core/src/option_ext.rs", vec![def("unwrap_or_else", Kind::Method, 12, 6).parent("OptionExt")]);
    put(
        &mut idx,
        "crates/layout/src/widgets/data_table.rs",
        vec![def("DataTable", Kind::Struct, 100, 30), def("DataTableCell", Kind::Struct, 254, 9), def("render", Kind::Method, 140, 12).parent("DataTable")],
    );
    put(
        &mut idx,
        "crates/layout/src/managers/scroll_state.rs",
        vec![def("ScrollManager", Kind::Struct, 609, 123), def("scroll_to", Kind::Method, 700, 10).parent("ScrollManager")],
    );
    put(&mut idx, "crates/speech/src/session.rs", vec![def("SpeechSession", Kind::Struct, 12, 40), def("listen", Kind::Method, 60, 10).parent("SpeechSession")]);
    put(&mut idx, "examples/azul-review/src/lib.rs", vec![def("AppState", Kind::Struct, 5, 30)]);
    put(
        &mut idx,
        "examples/hello/src/main.rs",
        vec![
            def("HelloWorld", Kind::Struct, 3, 5),
            def("test", Kind::Fn, 10, 3),
            def("UserName", Kind::Struct, 14, 4),
            def("ErrorMessage", Kind::Struct, 20, 4),
            def("NoOp", Kind::Struct, 26, 2),
            def("main", Kind::Fn, 30, 5),
        ],
    );
    let mut tests = Scanned { size: 50, lines: 20, test_file: true, defs: vec![def("a_dom_can_be_cloned", Kind::Fn, 3, 6), def("fixture_in_tests", Kind::Fn, 12, 4)], ..Scanned::default() };
    tests.lang = None;
    idx.put_scanned("crates/core/src/dom_test.rs", tests);
    idx
}

fn heard(text: &str) -> Vec<HeardWord> {
    text.split_whitespace().map(|w| HeardWord { text: w.to_string(), probability: 0.9, biased: false }).collect()
}

fn resolve(idx: &Index, text: &str) -> Resolution {
    Resolver::new(idx).resolve(&heard(text), &Context::default())
}

fn resolve_in(idx: &Index, text: &str, file: &str) -> Resolution {
    Resolver::new(idx).resolve(&heard(text), &Context { file: Some(file), recent: &[] })
}

fn names(r: &Resolution) -> Vec<&str> {
    r.certain.iter().map(|c| c.def.name.as_str()).collect()
}

// ---- the user's own sentence ----

#[test]
fn the_identifier_the_recogniser_wrote_is_certain_and_the_sentence_stays_as_it_was() {
    let idx = index();
    let r = resolve(&idx, "yeah we had a CssError here");
    assert_eq!(names(&r), ["CssError"]);
    let c = &r.certain[0];
    assert_eq!(c.words, 4..5);
    assert_eq!((c.def.file.as_str(), c.def.line, c.def.kind), ("crates/css/src/error.rs", 10, Kind::Struct));
    assert_ne!(c.via & VIA_IDENTIFIER, 0);
    assert_eq!(r.corrected, "yeah we had a CssError here");
    assert_eq!(r.raw, "yeah we had a CssError here");
    assert!(r.hints.is_empty(), "{:?}", r.hints);
}

#[test]
fn the_same_sentence_as_a_recogniser_writes_it_in_six_ways_is_the_same_certain_mention() {
    let idx = index();
    for said in [
        "yeah we had a see esses error here",
        "yeah we had a C S S error here",
        "yeah we had a sea ess ess error here",
        "yeah we had a CSS error here",
        "yeah we had a C-S-S error here",
        "yeah we had a see ess ess error here",
    ] {
        let r = resolve(&idx, said);
        assert_eq!(names(&r), ["CssError"], "{said}: {r:?}");
        assert_eq!(r.corrected, "yeah we had a CssError here", "{said}");
        assert_eq!(r.raw, said);
    }
    let r = resolve(&idx, "yeah we had a see esses error here");
    assert_ne!(r.certain[0].via & VIA_LETTERS, 0);
    assert!(!r.certain[0].repaired);
}

#[test]
fn punctuation_around_the_words_stays_around_the_name() {
    let idx = index();
    let r = resolve(&idx, "so (CSS error), then");
    assert_eq!(names(&r), ["CssError"]);
    assert_eq!(r.corrected, "so (CssError), then");
}

#[test]
fn a_name_that_is_only_nearly_what_was_heard_is_a_hint_and_never_a_found_definition() {
    let idx = index();
    let r = resolve(&idx, "yeah we had a sees error here");
    assert!(r.certain.is_empty(), "{:?}", r.certain);
    assert_eq!(r.corrected, r.raw);
    assert_eq!(r.hints.len(), 1, "{:?}", r.hints);
    let h = &r.hints[0];
    assert!(h.patterns.iter().any(|p| p == "css.?error"), "{:?}", h.patterns);
    assert!(h.similar.iter().any(|s| s == "CssError"), "{:?}", h.similar);
    assert!(h.candidates.is_empty(), "a hint's similar names are not definitions");
}

#[test]
fn a_word_the_recogniser_doubted_next_to_a_spelled_acronym_is_repaired_a_sure_word_is_not() {
    let idx = index();
    let mut words = heard("we had to see ess ess ever here");
    words[6].probability = 0.13;
    let r = Resolver::new(&idx).resolve(&words, &Context::default());
    assert_eq!(names(&r), ["CssError"], "{r:?}");
    assert!(r.certain[0].repaired);
    assert_eq!(r.corrected, "we had to CssError here");
    // The same words, all of them sure: nothing is repaired.
    words[6].probability = 0.9;
    let r = Resolver::new(&idx).resolve(&words, &Context::default());
    assert!(r.certain.is_empty(), "{:?}", r.certain);
    // A probability that is not known is not doubt.
    words[6].probability = 0.0;
    let r = Resolver::new(&idx).resolve(&words, &Context::default());
    assert!(r.certain.is_empty(), "{:?}", r.certain);
}

// ---- names said as words ----

#[test]
fn names_said_as_words_are_found_when_the_words_are_the_names_words() {
    let idx = index();
    let cases: [(&str, &str, Kind); 12] = [
        ("the data table cell renders the wrong padding", "DataTableCell", Kind::Struct),
        ("the scroll manager is stale", "ScrollManager", Kind::Struct),
        ("the speech session is closed twice", "SpeechSession", Kind::Struct),
        ("the struct dom is wrong", "Dom", Kind::Struct),
        ("az review opens folders", "AzReview", Kind::Package),
        ("the app state is empty", "AppState", Kind::Struct),
        ("see esses parse error is an enum", "CssParseError", Kind::Enum),
        ("the hdp request failed", "HttpRequest", Kind::Struct),
        ("the age ttp request failed", "HttpRequest", Kind::Struct),
        ("the jason parser breaks", "JsonParser", Kind::Struct),
        ("utf eight string is wrong", "Utf8String", Kind::Struct),
        ("sha two fifty six is slow", "Sha256", Kind::Struct),
    ];
    for (said, name, kind) in cases {
        let r = resolve(&idx, said);
        assert_eq!(names(&r), [name], "{said}: {r:?}");
        assert_eq!(r.certain[0].def.kind, kind, "{said}");
    }
}

#[test]
fn a_mishearing_of_a_code_word_is_a_reading_that_counts_as_evidence() {
    let idx = index();
    let r = resolve(&idx, "the hdp request failed");
    assert_ne!(r.certain[0].via & VIA_HOMOPHONE, 0);
}

#[test]
fn the_longest_name_wins_over_the_name_inside_it() {
    let idx = index();
    let r = resolve(&idx, "the data table cell is blank");
    assert_eq!(names(&r), ["DataTableCell"]);
    let r = resolve(&idx, "the data table struct is blank");
    assert_eq!(names(&r), ["DataTable"]);
}

#[test]
fn a_dependency_said_aloud_is_found_in_the_package_the_reviewer_is_in() {
    let idx = index();
    let r = resolve_in(&idx, "we use serde here", "crates/css/src/props.rs");
    assert_eq!(names(&r), ["serde"]);
    assert_eq!(r.certain[0].def.file, "crates/css/Cargo.toml");
    assert_eq!(r.certain[0].scope, Scope::Crate);
    // Away from both, the first manifest that has it is a place to look, not the only one.
    let r = resolve(&idx, "we use serde here");
    assert_eq!(names(&r), ["serde"]);
    assert_eq!(r.certain[0].scope, Scope::Workspace);
}

#[test]
fn a_german_sentence_finds_the_name_like_an_english_one() {
    let idx = index();
    let r = resolve(&idx, "wir haben hier einen CssError gesehen");
    assert_eq!(names(&r), ["CssError"]);
    let r = resolve(&idx, "wir haben hier einen see esses error gesehen");
    assert_eq!(names(&r), ["CssError"]);
}

// ---- one word needs more than its spelling ----

#[test]
fn a_common_word_that_is_a_name_is_not_found_without_a_kind_word_or_the_recognisers_bias() {
    let idx = index();
    assert!(resolve(&idx, "the user name is empty").certain.is_empty());
    assert!(resolve(&idx, "an error message appears").certain.is_empty());
    let mut words = heard("the user name is empty");
    words[1].biased = true;
    let r = Resolver::new(&idx).resolve(&words, &Context::default());
    assert_eq!(names(&r), ["UserName"], "a word the recogniser was told to favour is evidence");
    let r = resolve(&idx, "the struct user name is empty");
    assert_eq!(names(&r), ["UserName"]);
}

#[test]
fn a_name_in_more_places_than_one_is_found_in_the_file_or_the_package_or_not_at_all() {
    let idx = index();
    // Two enums called Error: no context, no kind word that tells: candidates, never a pick.
    let r = resolve(&idx, "the enum error is not handled");
    assert!(r.certain.is_empty(), "{:?}", r.certain);
    assert_eq!(r.hints.len(), 1);
    assert_eq!(r.hints[0].candidates.len(), 2);
    assert!(r.hints[0].kinds.contains(&Kind::Enum));
    // In the file of one of them, that one; in the package of one, that one.
    let r = resolve_in(&idx, "the enum error is not handled", "crates/css/src/props.rs");
    assert_eq!((names(&r), r.certain[0].scope), (vec!["Error"], Scope::File));
    assert_eq!(r.certain[0].def.file, "crates/css/src/props.rs");
    let r = resolve_in(&idx, "the enum error is not handled", "crates/core/src/dom.rs");
    assert_eq!((names(&r), r.certain[0].scope), (vec!["Error"], Scope::Crate));
    assert_eq!(r.certain[0].def.file, "crates/core/src/error.rs");
}

#[test]
fn a_kind_word_picks_the_methods_among_names_and_the_file_narrows_them() {
    let idx = index();
    let r = resolve(&idx, "the method new is slow");
    assert!(r.certain.is_empty());
    assert_eq!(r.hints[0].candidates.len(), 2);
    let r = resolve_in(&idx, "the method new is slow", "crates/core/src/ui/view.rs");
    assert_eq!(r.certain[0].def.parent.as_deref(), Some("View"));
    assert_eq!(r.certain[0].scope, Scope::File);
}

#[test]
fn what_was_found_before_in_this_conversation_breaks_a_tie() {
    let idx = index();
    let recent: Vec<DefInfo> = idx.lookup(&["error"]).into_iter().filter(|d| d.file.starts_with("crates/css")).collect();
    let r = Resolver::new(&idx).resolve(&heard("the enum error is not handled"), &Context { file: None, recent: &recent });
    assert_eq!(names(&r), ["Error"]);
    assert_eq!(r.certain[0].scope, Scope::Recent);
    assert_eq!(r.certain[0].def.file, "crates/css/src/props.rs");
}

#[test]
fn a_name_only_tests_have_is_found_only_in_the_file_that_has_it() {
    let idx = index();
    let r = resolve(&idx, "the fixture in tests helper is odd");
    assert!(r.certain.is_empty(), "{:?}", r.certain);
    let r = resolve_in(&idx, "the fixture in tests helper is odd", "crates/core/src/dom_test.rs");
    assert_eq!(names(&r), ["fixture_in_tests"]);
}

#[test]
fn a_name_that_is_part_of_a_longer_name_that_begins_with_a_joining_word_is_not_certain() {
    let idx = index();
    // `get margin top` may be `margin_top` or `get_margin_top`: both are told, none is found.
    let r = resolve(&idx, "the get margin top is odd");
    assert!(r.certain.is_empty(), "{:?}", r.certain);
    assert_eq!(r.hints.len(), 1, "{:?}", r.hints);
    let named: Vec<&str> = r.hints[0].candidates.iter().map(|d| d.name.as_str()).collect();
    assert!(named.contains(&"get_margin_top") && named.contains(&"margin_top"), "{named:?}");
    // Said without the joining word it is the shorter name, and with a kind word the longer.
    assert_eq!(names(&resolve(&idx, "the margin top is odd")), ["margin_top"]);
    assert_eq!(names(&resolve(&idx, "the function get margin top is odd")), ["get_margin_top"]);
}

#[test]
fn a_kind_word_that_is_part_of_a_longer_name_does_not_make_the_shorter_name_certain() {
    let idx = index();
    // `input class` may be `Input` (a class) or `InputClass`.
    let r = resolve(&idx, "the input class is odd");
    assert!(r.certain.is_empty(), "{:?}", r.certain);
    let named: Vec<&str> = r.hints.iter().flat_map(|h| h.candidates.iter().map(|d| d.name.as_str())).collect();
    assert!(named.contains(&"Input") && named.contains(&"InputClass"), "{named:?}");
}

#[test]
fn a_name_the_recogniser_wrote_as_one_lower_case_word_is_found_when_it_is_more_than_everyday_words() {
    let idx = index();
    // Whistle writes a favoured compound in lower case.
    assert_eq!(names(&resolve(&idx, "the scrollmanager is stale")), ["ScrollManager"]);
    assert_eq!(names(&resolve(&idx, "the datatablecell is blank")), ["DataTableCell"]);
    assert_eq!(names(&resolve(&idx, "a speechsession was closed")), ["SpeechSession"]);
    // Pieces that are not the name's words but make its letters.
    assert_eq!(names(&resolve(&idx, "we call unwrapor else here")), ["unwrap_or_else"]);
    // `username` is an everyday word that is also a name: not found by its spelling.
    assert!(resolve(&idx, "what is the username here").certain.is_empty());
    // Too short to be the name of anything.
    assert!(resolve(&idx, "the dom is stale").certain.is_empty());
}

#[test]
fn a_window_of_everyday_words_with_a_word_that_is_only_nearly_a_name_is_not_a_hint() {
    let idx = index();
    for said in ["that was a good session", "a lot of people use json", "the web request took long", "the node is empty"] {
        let r = resolve(&idx, said);
        assert!(r.certain.is_empty() && r.hints.is_empty(), "{said}: {r:?}");
    }
}

#[test]
fn a_single_word_with_nothing_that_says_the_code_is_meant_gives_no_hint() {
    let idx = index();
    for said in [
        "the node is empty",
        "she gave a speech yesterday",
        "that was a good session",
        "the web request took long",
        "what is an app",
        "a lot of people use json",
        "we are going to render it later",
    ] {
        let r = resolve(&idx, said);
        assert!(r.certain.is_empty() && r.hints.is_empty(), "{said}: {r:?}");
    }
}

// ---- acronyms in the corrected text ----

#[test]
fn a_spelled_out_acronym_that_is_no_name_is_written_as_the_acronym_in_the_corrected_text() {
    let idx = index();
    let r = resolve(&idx, "we use aitch tee tee pee here");
    assert!(r.certain.is_empty());
    assert_eq!(r.corrected, "we use HTTP here");
    assert_eq!(r.raw, "we use aitch tee tee pee here");
    assert_eq!(r.corrections.len(), 1);
    assert_eq!(r.corrections[0].kind, CorrectionKind::Acronym);
    // Words that only could be letters stay words.
    let r = resolve(&idx, "are you ok see you later");
    assert_eq!(r.corrected, r.raw);
}

// ---- what must find nothing ----

const NEGATIVES: [&str; 48] = [
    "hello world this is a test",
    "yes sir",
    "see us error",
    "we can see errors in the log",
    "the error is wrong",
    "he was awake all night",
    "the enemy is in the room",
    "no op here",
    "read me first",
    "what is the user name",
    "an error message appears",
    "we should test this",
    "yeah that makes sense",
    "I think so",
    "let us look at the file",
    "this is the main thing",
    "okay next",
    "so basically the problem is here",
    "wir haben das schon gesehen",
    "ja genau das ist es",
    "ich glaube das stimmt nicht",
    "can you open the file",
    "see you tomorrow",
    "are you ready",
    "why is it slow",
    "the data is wrong",
    "the node is empty",
    "we need a new view",
    "the length is wrong here",
    "oh i see what you mean",
    "you are right about the parse",
    "it is a bit slow today",
    "the color is too dark",
    "we are going to render it later",
    "that was a good session",
    "she gave a speech yesterday",
    "I will manage the scroll myself",
    "the cell phone is on the table",
    "can we talk about the state of things",
    "what is an app",
    "the web request took long",
    "a lot of people use json",
    "my name is not important",
    "hello hello hello",
    "um uh yeah well",
    "",
    "   ",
    "the the the the",
];

#[test]
fn ordinary_sentences_resolve_to_nothing_for_certain() {
    let idx = index();
    let mut hints = 0;
    for said in NEGATIVES {
        let r = resolve(&idx, said);
        assert!(r.certain.is_empty(), "{said:?} found {:?}", r.certain);
        assert_eq!(r.corrected, r.raw, "{said:?}");
        hints += r.hints.len();
        assert!(r.hints.len() <= 1, "{said:?} gave {} hints", r.hints.len());
    }
    // Hints on the ordinary sentences are rare: at most one in ten.
    assert!(hints * 10 <= NEGATIVES.len(), "{hints} hints on {} sentences", NEGATIVES.len());
}

#[test]
fn the_same_ordinary_sentences_with_the_files_of_a_package_around_them_still_find_nothing() {
    let idx = index();
    for file in ["crates/css/src/props.rs", "examples/hello/src/main.rs", "crates/core/src/dom.rs"] {
        for said in NEGATIVES {
            let r = resolve_in(&idx, said, file);
            assert!(r.certain.is_empty(), "{said:?} in {file} found {:?}", r.certain);
        }
    }
}

// ---- bounds ----

#[test]
fn hostile_input_is_bounded_and_never_panics() {
    let idx = index();
    let started = std::time::Instant::now();
    let many = "see ess ess error and the data table cell ".repeat(500);
    let r = resolve(&idx, &many);
    assert!(r.certain.len() <= MAX_MENTIONS);
    assert!(r.hints.len() <= Options::default().max_hints);
    // What is past the words that are read stays in the corrected sentence as heard.
    let past = format!("{}see esses error", "word ".repeat(450));
    let r = resolve(&idx, &past);
    assert_eq!(r.corrected, r.raw, "nothing was corrected in the first 400 words and the rest is kept: {} vs {}", r.corrected.len(), r.raw.len());
    let long_word = "x".repeat(1_000_000);
    let _ = resolve(&idx, &long_word);
    let _ = resolve(&idx, &format!("the {} error", "c ".repeat(20_000)));
    let _ = resolve(&idx, "ü ß 日本語 😀 \u{0} \t");
    let _ = resolve(&idx, "- . - . ,,, ;;; ---");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
    let none = Resolver::new(&Index::new(Caps::default())).resolve(&heard("see esses error"), &Context::default());
    assert!(none.certain.is_empty() && none.hints.is_empty());
}

#[test]
fn the_answer_is_the_same_every_time_and_a_hint_is_short() {
    let idx = index();
    for said in ["yeah we had a sees error here", "the enum error is not handled", "the struct css errr is odd", "the method new is slow"] {
        let a = resolve(&idx, said);
        let b = resolve(&idx, said);
        assert_eq!(a, b, "{said}");
        for h in &a.hints {
            assert!(h.patterns.len() <= 3, "{h:?}");
            assert!(h.candidates.len() <= 3, "{h:?}");
            assert!(h.similar.len() <= 3, "{h:?}");
            assert!(h.said.chars().count() <= 80, "{h:?}");
            assert!(h.folders.len() <= 3, "{h:?}");
        }
    }
}

#[test]
fn a_word_after_a_kind_word_that_is_no_name_yet_is_what_to_grep_for() {
    let idx = index();
    let r = resolve(&idx, "the struct css errr is odd");
    assert!(r.certain.is_empty());
    assert_eq!(r.hints.len(), 1, "{:?}", r.hints);
    assert!(r.hints[0].kinds.contains(&Kind::Struct));
    assert!(r.hints[0].patterns.iter().any(|p| p == "css.?errr"), "{:?}", r.hints[0].patterns);
    assert!(r.hints[0].similar.iter().any(|s| s == "CssError"), "{:?}", r.hints[0].similar);
}

#[test]
fn the_precision_and_the_recall_of_the_fixtures_are_counted() {
    let idx = index();
    let positives: [(&str, &str); 6] = [
        ("yeah we had a CssError here", "CssError"),
        ("yeah we had a see esses error here", "CssError"),
        ("the scroll manager is stale", "ScrollManager"),
        ("the hdp request failed", "HttpRequest"),
        ("utf eight string is wrong", "Utf8String"),
        ("sha two fifty six is slow", "Sha256"),
    ];
    let found = positives.iter().filter(|(said, name)| names(&resolve(&idx, said)).contains(name)).count();
    let false_certain = NEGATIVES.iter().filter(|said| !resolve(&idx, said).certain.is_empty()).count();
    assert_eq!(found, positives.len(), "recall");
    assert_eq!(false_certain, 0, "false Certain on {} ordinary sentences", NEGATIVES.len());
}

// ---- the real answers of two recognisers ----

/// What two recognisers wrote for the sentences of the speech agent's vocabulary matrix, spoken by three
/// voices: `clip<TAB>engine and variant tags<TAB>word|probability|biased ...`.
const MATRIX: &str = include_str!("symbols_fixtures/heard_matrix.tsv");

struct Answer {
    clip: String,
    tags: Vec<String>,
    words: Vec<HeardWord>,
}

fn matrix() -> Vec<Answer> {
    MATRIX
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let clip = fields.next().unwrap_or("").to_string();
            let tags = fields.next().unwrap_or("").split(',').map(str::to_string).collect();
            let words = fields
                .map(|f| {
                    let mut p = f.rsplitn(3, '|');
                    let biased = p.next() == Some("1");
                    let probability = p.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
                    HeardWord { text: p.next().unwrap_or("").to_string(), probability, biased }
                })
                .collect();
            Answer { clip, tags, words }
        })
        .collect()
}

/// The names a clip's sentence says (`1_Daniel` is the clip `1` said by Daniel).
fn said_names(clip: &str) -> &'static [&'static str] {
    match clip.split('_').next().unwrap_or("") {
        "1" | "1c" => &["CssError"],
        "2" => &["SpeechSession"],
        "5" | "5b" | "5c" => &["AzReview"],
        "6" => &["DataTableCell"],
        // "the JSON and the HTML in the DOM": the workspace has a `Dom`.
        "7" => &["Dom"],
        "9" => &["unwrap_or_else"],
        "10" => &["ScrollManager", "HttpRequest"],
        _ => &[],
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    right: usize,
    hint: usize,
    miss: usize,
    wrong: usize,
    false_certain: usize,
}

fn count(answers: &[Answer], keep: impl Fn(&Answer) -> bool) -> Counts {
    let idx = index();
    let mut c = Counts::default();
    for a in answers.iter().filter(|a| keep(a)) {
        let r = Resolver::new(&idx).resolve(&a.words, &Context::default());
        let want = said_names(&a.clip);
        if want.is_empty() {
            c.false_certain += r.certain.len();
            for m in &r.certain {
                println!("  certain on {} {:?}: {} <- {:?}", a.clip, a.tags, m.def.name, a.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>());
            }
            continue;
        }
        c.wrong += r.certain.iter().filter(|m| !want.contains(&m.def.name.as_str())).count();
        for name in want {
            if r.certain.iter().any(|m| m.def.name == *name) {
                c.right += 1;
            } else if r.hints.iter().any(|h| h.similar.iter().any(|s| s == name) || h.candidates.iter().any(|d| d.name == *name)) {
                c.hint += 1;
            } else {
                c.miss += 1;
                println!("  miss {name} on {} {:?}: {:?}", a.clip, a.tags, a.words.iter().map(|w| format!("{}:{}{}", w.text, w.probability, if w.biased { "*" } else { "" })).collect::<Vec<_>>().join(" "));
            }
        }
    }
    c
}

#[test]
fn the_real_answers_of_the_recognisers_never_make_a_wrong_name_certain() {
    let answers = matrix();
    assert!(answers.len() > 200, "{} answers", answers.len());
    let all = count(&answers, |_| true);
    assert_eq!((all.wrong, all.false_certain), (0, 0), "{all:?}");
}

#[test]
fn the_real_answers_of_each_recogniser_with_the_realistic_vocabulary_are_counted() {
    let answers = matrix();
    // V7 is the vocabulary the app passes: the identifiers, the spoken parts, the acronyms, the keywords.
    let whistle = count(&answers, |a| a.tags.iter().any(|t| t == "wV7"));
    let native = count(&answers, |a| a.tags.iter().any(|t| t == "nV7"));
    let none_whistle = count(&answers, |a| a.tags.iter().any(|t| t == "wV0"));
    let none_native = count(&answers, |a| a.tags.iter().any(|t| t == "nV0"));
    println!("Whistle, vocabulary V7:  {whistle:?}");
    println!("system,  vocabulary V7:  {native:?}");
    println!("Whistle, no vocabulary:  {none_whistle:?}");
    println!("system,  no vocabulary:  {none_native:?}");
    for c in [&whistle, &native, &none_whistle, &none_native] {
        assert_eq!((c.wrong, c.false_certain), (0, 0), "{c:?}");
    }
    // What the three voices get right for certain, at the least (the numbers of this version).
    assert!(whistle.right >= 24 && whistle.right + whistle.hint >= 26, "{whistle:?}");
    assert!(native.right >= 21, "{native:?}");
}

#[test]
fn what_the_hard_voice_wrote_is_still_found_when_the_recogniser_doubted_the_word() {
    let idx = index();
    let answers = matrix();
    // Fred: "yeah we had a C S S error here" came out as "Yell we had to see S. S. ever here."
    let fred = answers.iter().find(|a| a.clip == "1_Fred" && a.tags.iter().any(|t| t == "wV7")).expect("in the fixture");
    let r = Resolver::new(&idx).resolve(&fred.words, &Context::default());
    assert_eq!(names(&r), ["CssError"], "{:?}", fred.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>());
    assert!(r.certain[0].repaired);
}
