//! What the agent is told about the code the reviewer named: the definitions found (with their doc
//! comment and their lines, within a budget), where the rest is, what to grep for the names that
//! are only hinted at, and how the words were heard. No window, no azul.

use std::{sync::Arc, time::Duration};

use super::*;
use crate::{
    symbols::{DefInfo, Kind, Vis},
    symbols_resolve::{Certain, Context, Correction, CorrectionKind, Hint, Resolution, Resolver, Scope},
    symbols_sched::{ActivityGate, Indexer, IndexerSettings},
    symbols_source::MemSource,
    symbols_spoken::{HeardWord, VIA_HOMOPHONE, VIA_LETTERS},
    symbols_vocab::{build as build_vocab, VocabContext, VocabOptions},
};

const ERROR_RS: &str = "\
use std::fmt;

/// What went wrong while a style was read.
///
/// Kept with the line it happened on.
#[derive(Debug, Clone)]
pub struct CssError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for CssError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, \"{}\", self.message)
    }
}
";

fn def(name: &str, kind: Kind, file: &str, line: u32, lines: u32) -> DefInfo {
    DefInfo { name: name.to_string(), kind, vis: Vis::Public, test: false, file: file.to_string(), line, lines, parent: None }
}

fn certain(def: DefInfo, words: std::ops::Range<usize>, via: u8, scope: Scope, score: f32) -> Certain {
    Certain { words, def, scope, via, repaired: false, score }
}

fn source() -> MemSource {
    let mut m = MemSource::new();
    m.put("crates/css/src/error.rs", ERROR_RS.as_bytes().to_vec());
    m.put("crates/css/src/long.rs", (1..=200).map(|i| format!("line {i} of a long function\n")).collect::<String>().into_bytes());
    m.put("py/tool.py", b"import os\n\n# The one that reads.\n@cached\ndef read_all(path):\n    return path\n\nx = 1\n".to_vec());
    m
}

fn resolution(certain: Vec<Certain>, hints: Vec<Hint>, raw: &str, corrected: &str) -> Resolution {
    let corrections = certain
        .iter()
        .map(|c| Correction { words: c.words.clone(), from: raw.to_string(), to: c.def.name.clone(), kind: CorrectionKind::Symbol })
        .collect();
    Resolution { certain, hints, corrections, raw: raw.to_string(), corrected: corrected.to_string() }
}

#[test]
fn a_found_definition_comes_with_its_doc_comment_its_lines_and_how_it_was_found() {
    let m = source();
    let c = certain(def("CssError", Kind::Struct, "crates/css/src/error.rs", 7, 4), 4..6, VIA_LETTERS, Scope::Workspace, 0.9);
    let r = resolution(vec![c], vec![], "see esses error", "CssError");
    let s = build_section(&r, &m, &ContextBudget::default());
    assert_eq!(s.snippets.len(), 1);
    let sn = &s.snippets[0];
    assert_eq!((sn.name.as_str(), sn.kind, sn.file.as_str()), ("CssError", Kind::Struct, "crates/css/src/error.rs"));
    assert_eq!((sn.line_start, sn.line_end), (3, 10));
    let lines: Vec<&str> = sn.text.lines().collect();
    assert_eq!(lines[0], "/// What went wrong while a style was read.");
    assert!(lines.contains(&"#[derive(Debug, Clone)]"));
    assert!(lines.contains(&"pub struct CssError {"));
    assert_eq!(lines.last().copied(), Some("}"));
    assert!(!sn.text.contains("impl fmt::Display"), "only the definition's own lines");
    assert!(sn.why.contains("see esses error") && sn.why.contains("spelled"), "{}", sn.why);
    assert!(s.pointers.is_empty() && s.stale.is_empty());
}

#[test]
fn what_a_mention_was_found_by_is_told_in_a_few_words() {
    let m = source();
    let mk = |via, scope, repaired| {
        let mut c = certain(def("CssError", Kind::Struct, "crates/css/src/error.rs", 7, 4), 0..1, via, scope, 0.5);
        c.repaired = repaired;
        build_section(&resolution(vec![c], vec![], "x", "CssError"), &m, &ContextBudget::default()).snippets.remove(0).why
    };
    assert!(mk(VIA_HOMOPHONE, Scope::File, false).contains("mishearing") && mk(VIA_HOMOPHONE, Scope::File, false).contains("this file"));
    assert!(mk(0, Scope::Crate, false).contains("package"));
    assert!(mk(0, Scope::Recent, false).contains("before"));
    assert!(mk(VIA_LETTERS, Scope::Workspace, true).contains("doubted"));
}

#[test]
fn a_long_definition_is_cut_at_the_budget_of_lines_and_says_so() {
    let m = source();
    let c = certain(def("long", Kind::Fn, "crates/css/src/long.rs", 10, 150), 0..1, 0, Scope::Workspace, 0.7);
    let s = build_section(&resolution(vec![c], vec![], "long", "long"), &m, &ContextBudget { max_lines: 12, ..ContextBudget::default() });
    let sn = &s.snippets[0];
    assert_eq!(sn.text.lines().count(), 13, "{}", sn.text);
    assert!(sn.text.lines().last().unwrap().contains("more lines"), "{}", sn.text);
    assert_eq!((sn.line_start, sn.line_end), (10, 159));
}

#[test]
fn what_does_not_fit_the_budget_is_a_pointer_and_the_best_one_always_has_its_text() {
    let m = source();
    let a = certain(def("CssError", Kind::Struct, "crates/css/src/error.rs", 7, 4), 0..1, 0, Scope::Workspace, 0.9);
    let b = certain(def("long", Kind::Fn, "crates/css/src/long.rs", 10, 30), 1..2, 0, Scope::Workspace, 0.5);
    let r = resolution(vec![b.clone(), a.clone()], vec![], "x y", "CssError long");
    let s = build_section(&r, &m, &ContextBudget { chars: 150, ..ContextBudget::default() });
    assert_eq!(s.snippets.len(), 1);
    assert_eq!(s.snippets[0].name, "CssError", "the better one first");
    assert_eq!(s.pointers.len(), 1);
    assert_eq!((s.pointers[0].name.as_str(), s.pointers[0].file.as_str(), s.pointers[0].line), ("long", "crates/css/src/long.rs", 10));
    // A budget smaller than the best one's text still tells the best one, cut.
    let tiny = build_section(&r, &m, &ContextBudget { chars: 20, ..ContextBudget::default() });
    assert_eq!(tiny.snippets.len(), 1);
    assert!(tiny.snippets[0].text.len() <= 20 + 4, "{}", tiny.snippets[0].text);
}

#[test]
fn a_definition_that_moved_or_is_gone_is_a_pointer_marked_stale_never_a_wrong_text() {
    let m = source();
    let moved = certain(def("CssError", Kind::Struct, "crates/css/src/error.rs", 8, 4), 0..1, 0, Scope::Workspace, 0.9);
    let gone = certain(def("Nothing", Kind::Struct, "crates/css/src/missing.rs", 3, 1), 1..2, 0, Scope::Workspace, 0.8);
    let past_end = certain(def("Far", Kind::Fn, "crates/css/src/error.rs", 9_999, 1), 2..3, 0, Scope::Workspace, 0.7);
    let s = build_section(&resolution(vec![moved, gone, past_end], vec![], "a b c", "A B C"), &m, &ContextBudget::default());
    assert!(s.snippets.is_empty(), "{:?}", s.snippets);
    let names: Vec<&str> = s.stale.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["CssError", "Nothing", "Far"]);
}

#[test]
fn two_mentions_of_one_definition_are_one_snippet() {
    let m = source();
    let d = def("CssError", Kind::Struct, "crates/css/src/error.rs", 7, 4);
    let r = resolution(vec![certain(d.clone(), 0..1, 0, Scope::Workspace, 0.9), certain(d, 3..4, 0, Scope::Workspace, 0.8)], vec![], "a b c d", "CssError b c CssError");
    assert_eq!(build_section(&r, &m, &ContextBudget::default()).snippets.len(), 1);
}

#[test]
fn the_comment_lines_and_decorators_of_another_language_come_with_it_and_a_blank_line_ends_them() {
    let m = source();
    let c = certain(def("read_all", Kind::Fn, "py/tool.py", 5, 2), 0..1, 0, Scope::Workspace, 0.9);
    let s = build_section(&resolution(vec![c], vec![], "read all", "read_all"), &m, &ContextBudget::default());
    assert_eq!(s.snippets[0].text, "# The one that reads.\n@cached\ndef read_all(path):\n    return path");
    assert_eq!(s.snippets[0].line_start, 3);
}

#[test]
fn a_file_that_is_not_text_or_a_line_that_is_endless_gives_a_pointer_or_a_cut_line() {
    let mut m = MemSource::new();
    m.put("bin.rs", vec![b'f', b'n', 0, 0, 0]);
    m.put("wide.rs", format!("fn wide() {{ {} }}\n", "x();".repeat(5_000)).into_bytes());
    let bin = certain(def("fn", Kind::Fn, "bin.rs", 1, 1), 0..1, 0, Scope::Workspace, 0.9);
    let wide = certain(def("wide", Kind::Fn, "wide.rs", 1, 1), 1..2, 0, Scope::Workspace, 0.8);
    let s = build_section(&resolution(vec![bin, wide], vec![], "a b", "A B"), &m, &ContextBudget::default());
    assert_eq!(s.snippets.len(), 1);
    assert!(s.snippets[0].text.chars().count() <= 260, "{}", s.snippets[0].text.chars().count());
    assert_eq!(s.stale.len(), 1);
}

fn hint(said: &str, patterns: &[&str], kinds: &[Kind]) -> Hint {
    Hint {
        words: 0..1,
        said: said.to_string(),
        patterns: patterns.iter().map(|p| (*p).to_string()).collect(),
        kinds: kinds.to_vec(),
        candidates: vec![],
        similar: vec![],
        folders: vec![],
    }
}

#[test]
fn a_hint_is_one_short_line_with_what_to_search_for_and_never_says_a_definition_was_found() {
    let m = source();
    let mut h = hint("sees error", &["css.?error", "sees.?error"], &[Kind::Struct]);
    h.similar = vec!["CssError".to_string()];
    h.folders = vec!["crates/css".to_string()];
    let mut amb = hint("error", &["error"], &[Kind::Enum]);
    amb.candidates = vec![def("Error", Kind::Enum, "crates/core/src/error.rs", 3, 8), def("Error", Kind::Enum, "crates/css/src/props.rs", 40, 6)];
    let s = build_section(&resolution(vec![], vec![h, amb], "a", "a"), &m, &ContextBudget::default());
    assert_eq!(s.hints.len(), 2);
    let first = &s.hints[0];
    assert!(!first.text.contains('\n'));
    for want in ["sees error", "css.?error", "struct", "CssError", "crates/css"] {
        assert!(first.text.contains(want), "{want} in {}", first.text);
    }
    assert!(first.text.chars().count() <= 220, "{}", first.text.chars().count());
    let second = &s.hints[1].text;
    assert!(second.contains("crates/core/src/error.rs:3") && second.contains("crates/css/src/props.rs:40"), "{second}");
}

#[test]
fn the_hints_are_capped_in_number_and_in_length() {
    let m = source();
    let many: Vec<Hint> = (0..9).map(|i| hint(&format!("thing {i}"), &["a.?b"; 3], &[])).collect();
    let s = build_section(&resolution(vec![], many, "a", "a"), &m, &ContextBudget { max_hints: 3, ..ContextBudget::default() });
    assert_eq!(s.hints.len(), 3);
    let mut long = hint(&"word ".repeat(200), &["pattern.?that.?is.?long"; 3], &[Kind::Fn]);
    long.similar = (0..3).map(|i| format!("AVeryLongSimilarName{i}")).collect();
    let s = build_section(&resolution(vec![], vec![long], "a", "a"), &m, &ContextBudget { max_hint_chars: 120, ..ContextBudget::default() });
    assert!(s.hints[0].text.chars().count() <= 120, "{}", s.hints[0].text.chars().count());
}

#[test]
fn the_prompt_part_has_a_section_for_what_there_is_and_nothing_for_what_there_is_not() {
    let m = source();
    let none = build_section(&resolution(vec![], vec![], "hello", "hello"), &m, &ContextBudget::default());
    assert!(none.is_empty());
    assert_eq!(none.render_prompt(), "");
    let c = certain(def("CssError", Kind::Struct, "crates/css/src/error.rs", 7, 4), 4..6, VIA_LETTERS, Scope::Workspace, 0.9);
    let only_certain = build_section(&resolution(vec![c.clone()], vec![], "see esses error", "CssError"), &m, &ContextBudget::default());
    let text = only_certain.render_prompt();
    assert!(text.contains("CssError (struct)") && text.contains("crates/css/src/error.rs:3-10"), "{text}");
    assert!(text.contains("pub struct CssError {"));
    assert!(text.contains("do not search"), "{text}");
    assert!(!text.contains("roughly"), "{text}");
    assert!(text.contains("heard as") && text.contains("see esses error"), "{text}");
    let both = build_section(&resolution(vec![c], vec![hint("sees error", &["css.?error"], &[])], "x", "x"), &m, &ContextBudget::default());
    let text = both.render_prompt();
    assert!(text.contains("roughly"), "{text}");
    assert!(!only_certain.is_empty() && only_certain.chars() > 100);
}

#[test]
fn the_words_as_heard_are_told_only_when_the_corrected_sentence_differs() {
    let m = source();
    let c = certain(def("CssError", Kind::Struct, "crates/css/src/error.rs", 7, 4), 0..1, 0, Scope::Workspace, 0.9);
    let same = build_section(&resolution(vec![c.clone()], vec![], "CssError", "CssError"), &m, &ContextBudget::default());
    assert!(!same.render_prompt().contains("heard as"), "{}", same.render_prompt());
    let differs = build_section(&resolution(vec![c], vec![], "yeah see esses error", "yeah CssError"), &m, &ContextBudget::default());
    let text = differs.render_prompt();
    assert!(text.contains("\"yeah see esses error\"") && text.contains("\"yeah CssError\""), "{text}");
}

#[test]
fn the_lines_around_a_place_are_numbered_and_stay_inside_the_file() {
    let m = source();
    let around = context_around(&m, "crates/css/src/error.rs", 7, 2).expect("there");
    assert_eq!(around.lines().count(), 5);
    assert!(around.lines().next().unwrap().starts_with("   5: "), "{around}");
    assert!(around.contains("   7: pub struct CssError {"), "{around}");
    let top = context_around(&m, "crates/css/src/error.rs", 1, 3).expect("there");
    assert!(top.lines().next().unwrap().starts_with("   1: "), "{top}");
    let bottom = context_around(&m, "crates/css/src/error.rs", 16, 10).expect("there");
    assert!(bottom.lines().last().unwrap().starts_with("  16: "), "{bottom}");
    assert!(context_around(&m, "crates/css/src/error.rs", 17, 2).is_none(), "the file has sixteen lines");
    assert!(context_around(&m, "crates/css/src/error.rs", 0, 2).is_none());
    assert!(context_around(&m, "crates/css/src/error.rs", 99, 2).is_none());
    assert!(context_around(&m, "nothing.rs", 1, 2).is_none());
    assert!(context_around(&m, "../etc/passwd", 1, 2).is_none());
}

#[test]
fn the_whole_flow_from_the_files_to_the_agents_prompt_and_the_recognisers_words_without_a_window() {
    // A workspace, indexed in the background outwards from the file the reviewer is looking at.
    let mut m = MemSource::new();
    m.put("crates/css/Cargo.toml", b"[package]\nname = \"azul-css\"\n[dependencies]\nserde = \"1\"\n".to_vec());
    m.put("crates/css/src/lib.rs", b"pub mod error;\n".to_vec());
    m.put("crates/css/src/error.rs", ERROR_RS.as_bytes().to_vec());
    let source = Arc::new(m);
    let indexer = Indexer::start(source.clone(), Some("crates/css/src/lib.rs"), IndexerSettings::default(), ActivityGate::new());
    assert!(indexer.wait_idle(Duration::from_secs(30)));
    // What the built-in recogniser wrote for "yeah we had a C S S error here" in the hardest voice, with its words' probabilities.
    let heard = |text: &str, p: f32| HeardWord { text: text.to_string(), probability: p, biased: false };
    let words = vec![
        heard("Yell", 0.7),
        heard("we", 0.73),
        heard("had", 0.97),
        heard("to", 0.99),
        heard("see", 0.99),
        heard("S.", 0.58),
        heard("S.", 0.84),
        heard("ever", 0.21),
        heard("here.", 0.94),
    ];
    let idx = indexer.index();
    let resolution = Resolver::new(&idx).resolve(&words, &Context { file: Some("crates/css/src/lib.rs"), recent: &[] });
    assert_eq!(resolution.corrected, "Yell we had to CssError here.");
    assert_eq!(resolution.raw, "Yell we had to see S. S. ever here.");
    assert_eq!(resolution.certain.len(), 1);
    assert!(resolution.certain[0].repaired);
    // What the agent is told.
    let prompt = build_section(&resolution, &*source, &ContextBudget::default()).render_prompt();
    assert!(prompt.contains("CssError (struct) crates/css/src/error.rs:3-10"), "{prompt}");
    assert!(prompt.contains("pub struct CssError {") && prompt.contains("/// What went wrong while a style was read."), "{prompt}");
    assert!(prompt.contains("heard as \"Yell we had to see S. S. ever here.\""), "{prompt}");
    // What the recogniser is told next: the names of the page, the file and the package, then the keywords.
    let v = build_vocab(&idx, &VocabContext { page_text: ERROR_RS, file: Some("crates/css/src/error.rs"), recent: &["CssError".to_string()] }, &VocabOptions::default());
    for want in ["CssError", "azul-css", "serde", "enum"] {
        assert!(v.words.iter().any(|w| w == want), "{want} in {:?}", v.words);
    }
    assert!(v.words.len() <= 200);
}
