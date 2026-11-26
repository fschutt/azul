//! The words: how a resource is read, how a template is filled, and that English and German have the same keys, the
//! same blanks, and every key the screens ask for.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::*;

// ---- reading a resource

#[test]
fn a_resource_is_key_equals_value_lines_with_comments_and_blank_lines_left_out() {
    let words = parse_ftl("# a comment\n\nhello = Hello\nwith-equals = a = b\ntight=tight\n  \n").unwrap();
    assert_eq!(words.get("hello").map(String::as_str), Some("Hello"));
    assert_eq!(words.get("with-equals").map(String::as_str), Some("a = b"));
    assert_eq!(words.get("tight").map(String::as_str), Some("tight"));
    assert_eq!(words.len(), 3);
}

#[test]
fn a_resource_that_cannot_be_read_says_which_line() {
    let say = |source: &str| parse_ftl(source).unwrap_err();
    assert_eq!(say("a = 1\na = 2\n"), FtlError { line: 2, message: "the key a is there twice".into() });
    assert_eq!(say("ok = 1\nno equals sign\n").line, 2);
    assert_eq!(say("= value\n").line, 1);
    assert_eq!(say("Bad Key = x\n").line, 1);
    assert_eq!(say("UPPER = x\n").line, 1);
    assert_eq!(say("empty =\n").line, 1);
    assert_eq!(say("empty =   \n").line, 1);
    assert_eq!(say("one = 1\n  continued on the next line\n").line, 2);
    assert!(say("x\n").to_string().starts_with("line 1: "));
}

// ---- filling a template

#[test]
fn a_template_is_filled_with_the_values_of_its_names() {
    assert_eq!(fill("Found { $mac }", &[("mac", "Studio Mac")]), "Found Studio Mac");
    assert_eq!(fill("{ $n } / { $max }", &[("max", "12"), ("n", "4")]), "4 / 12");
    assert_eq!(fill("{$n} tight", &[("n", "1")]), "1 tight");
    assert_eq!(fill("{ $n } and { $n }", &[("n", "x")]), "x and x");
    assert_eq!(fill("no names", &[("n", "x")]), "no names");
}

#[test]
fn a_name_with_no_value_shows_as_a_question_mark_and_other_braces_stay() {
    assert_eq!(fill("Hello { $who }", &[]), "Hello ?who");
    assert_eq!(fill("a { b } c", &[("b", "x")]), "a { b } c");
    assert_eq!(fill("open { $x", &[("x", "1")]), "open { $x");
    assert_eq!(fill("{ $", &[]), "{ $");
    assert_eq!(fill("{ $ }", &[]), "{ $ }");
}

#[test]
fn a_value_that_looks_like_a_name_is_not_filled_again() {
    assert_eq!(fill("{ $a }", &[("a", "{ $b }"), ("b", "no")]), "{ $b }");
}

#[test]
fn the_names_a_template_asks_for_come_in_order() {
    assert_eq!(placeholders("{ $n } of { $m } and { $n }"), ["n", "m", "n"]);
    assert!(placeholders("nothing").is_empty());
    assert_eq!(placeholders("{$tight}"), ["tight"]);
}

// ---- the shipped words

fn strings() -> Strings {
    Strings::from_sources(EN, DE).expect("the shipped resources are readable")
}

#[test]
fn the_shipped_resources_are_readable_and_not_empty() {
    let s = strings();
    assert!(s.keys(Lang::En).len() > 100);
    assert_eq!(Strings::builtin(), s);
}

#[test]
fn english_and_german_have_the_same_keys() {
    let s = strings();
    let en: BTreeSet<&str> = s.keys(Lang::En).into_iter().collect();
    let de: BTreeSet<&str> = s.keys(Lang::De).into_iter().collect();
    let only_en: Vec<&&str> = en.difference(&de).collect();
    let only_de: Vec<&&str> = de.difference(&en).collect();
    assert!(only_en.is_empty(), "missing in German: {only_en:?}");
    assert!(only_de.is_empty(), "missing in English: {only_de:?}");
}

#[test]
fn both_languages_fill_in_the_same_names() {
    let s = strings();
    for key in s.keys(Lang::En) {
        let en = placeholders(s.template(Lang::En, key).unwrap());
        let de = placeholders(s.template(Lang::De, key).unwrap());
        let (mut en_sorted, mut de_sorted) = (en.clone(), de.clone());
        en_sorted.sort();
        de_sorted.sort();
        assert_eq!(en_sorted, de_sorted, "{key}: English asks for {en:?}, German for {de:?}");
    }
}

#[test]
fn german_is_not_a_copy_of_english() {
    let s = strings();
    let keys = s.keys(Lang::En);
    let same: Vec<&str> = keys.iter().copied().filter(|k| s.template(Lang::En, k) == s.template(Lang::De, k)).collect();
    assert!(same.len() * 100 <= keys.len() * 15, "{} of {} words are the same in both languages: {same:?}", same.len(), keys.len());
}

#[test]
fn a_word_that_is_missing_shows_its_key_in_brackets() {
    let s = strings();
    assert_eq!(s.t(Lang::En, "no-such-key"), "[no-such-key]");
    assert_eq!(s.tf(Lang::De, "pair-found-title", &[("mac", "Büro-Mac")]), "Büro-Mac gefunden");
    assert_eq!(s.tf(Lang::En, "remote-page", &[("n", "2"), ("m", "5")]), "Page 2 of 5");
}

// ---- the sources use the words that are there

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the source folder") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The text of every source file that is not a test and not this one.
fn sources() -> String {
    let mut files = Vec::new();
    rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    files
        .iter()
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !name.ends_with("_test.rs") && name != "l10n.rs"
        })
        .map(|p| std::fs::read_to_string(p).expect("a source file"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The families of keys a source builds with `format!` (their members are written out in the resources only).
const BUILT_KEYS: [&str; 3] = ["fail-", "button-state-", "files-state-"];

/// The first words of the keys.
const FAMILIES: [&str; 10] = ["nav-", "tab-", "conn-", "pair-", "fail-", "remote-", "button-", "files-", "editor-", "settings-"];

#[test]
fn every_word_the_screens_ask_for_is_there_in_both_languages() {
    let s = strings();
    let text = sources();
    let mut asked = BTreeSet::new();
    for call in [".t(", ".tf("] {
        for part in text.split(call).skip(1) {
            if let Some(rest) = part.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    asked.insert(rest[..end].to_string());
                }
            }
        }
    }
    assert!(asked.len() > 40, "the scan found only {} asked words: {asked:?}", asked.len());
    for key in &asked {
        for lang in [Lang::En, Lang::De] {
            assert!(s.template(lang, key).is_some(), "{key} is asked for and {lang:?} has no such word");
        }
    }
}

#[test]
fn every_text_that_looks_like_a_key_is_one() {
    let s = strings();
    let text = sources();
    for part in text.split('"').skip(1).step_by(2) {
        let looks_like_a_key = part.len() < 60
            && FAMILIES.iter().any(|f| part.starts_with(f))
            && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !part.ends_with('-');
        if looks_like_a_key {
            assert!(s.template(Lang::En, part).is_some(), "\"{part}\" looks like a key and English has no such word");
        }
    }
}

#[test]
fn no_word_is_left_that_nothing_asks_for() {
    let s = strings();
    let text = sources();
    let unused: Vec<&str> = s
        .keys(Lang::En)
        .into_iter()
        .filter(|k| !text.contains(&format!("\"{k}\"")) && !BUILT_KEYS.iter().any(|p| k.starts_with(p)) && *k != "app-name")
        .collect();
    assert!(unused.is_empty(), "words nothing asks for: {unused:?}");
}

#[test]
fn the_failure_words_are_there_for_every_kind() {
    let s = strings();
    for kind in ["nopermission", "notfound", "updatecompanion", "updateazreview", "declined", "wrongcode", "expired", "forgotten", "busy", "timeout", "addresschanged"] {
        for part in ["title", "body"] {
            let key = format!("fail-{kind}-{part}");
            assert!(s.template(Lang::En, &key).is_some() && s.template(Lang::De, &key).is_some(), "{key}");
        }
    }
    for state in ["sending", "received", "working", "suggested", "done", "failed"] {
        let key = format!("button-state-{state}");
        assert!(s.template(Lang::En, &key).is_some() && s.template(Lang::De, &key).is_some(), "{key}");
    }
    for state in ["clean", "unresolved", "working"] {
        let key = format!("files-state-{state}");
        assert!(s.template(Lang::En, &key).is_some() && s.template(Lang::De, &key).is_some(), "{key}");
    }
}
