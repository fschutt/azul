//! The words of a test's desktop: what a headless run's `DesktopState` reads from the file
//! `AZ_DESKTOP_STATE_FILE` names. No desktop, no azul.

use super::*;

#[test]
fn a_script_says_the_app_in_front_the_pointer_the_lock_and_the_display() {
    let s = parse("front=com.apple.Safari|Safari\npointer=100.5,200\nlocked=1\nasleep=yes\n");
    assert_eq!(s.front, Some((String::from("com.apple.Safari"), String::from("Safari"))));
    assert_eq!(s.pointer, Some((100.5, 200.0)));
    assert!(s.locked && s.display_asleep);
    assert!(s.problems.is_empty(), "{:?}", s.problems);
}

#[test]
fn a_script_that_says_nothing_says_nothing_is_known_and_nothing_is_locked() {
    assert_eq!(parse(""), Script::default());
    let s = parse("# a comment\n\nfront=none\npointer=none\nlocked=0\nasleep=off\n");
    assert_eq!(s, Script::default());
}

#[test]
fn the_lock_and_the_sleep_take_the_words_of_a_switch_in_any_case() {
    for yes in ["1", "true", "TRUE", "Yes", "on", " on "] {
        assert!(parse(&format!("locked={yes}")).locked, "{yes}");
    }
    for no in ["0", "false", "No", "off"] {
        assert!(!parse(&format!("asleep={no}")).display_asleep, "{no}");
    }
    let s = parse("locked=maybe");
    assert!(!s.locked);
    assert_eq!(s.problems.len(), 1);
    assert!(s.problems[0].contains("locked") && s.problems[0].contains("maybe"), "{:?}", s.problems);
}

/// A bundle id without a name is its own name; a name may hold the separator; what is longer than the
/// caps is cut and said; a pointer that is no number, or no finite one, is nobody's.
///
/// Mutation: no cut (a name of a megabyte reaches the app); a pointer of NaN accepted.
#[test]
fn names_are_capped_and_a_pointer_must_be_a_pair_of_finite_numbers() {
    assert_eq!(parse("front=org.example.App").front, Some((String::from("org.example.App"), String::from("org.example.App"))));
    assert_eq!(parse("front=org.example.App|A|B").front, Some((String::from("org.example.App"), String::from("A|B"))));
    let long = "x".repeat(300);
    let s = parse(&format!("front=com.example.A|{long}"));
    assert_eq!(s.front.as_ref().unwrap().1.chars().count(), MAX_NAME);
    assert!(s.problems.iter().any(|p| p.contains("256")), "{:?}", s.problems);
    for bad in ["pointer=1", "pointer=a,b", "pointer=1,2,3", "pointer=NaN,1", "pointer=inf,0"] {
        let s = parse(bad);
        assert_eq!(s.pointer, None, "{bad}");
        assert_eq!(s.problems.len(), 1, "{bad}: {:?}", s.problems);
    }
    assert_eq!(parse("pointer= -3.5 , 7 ").pointer, Some((-3.5, 7.0)), "a pointer left of the primary display is negative");
}

/// A key that is none of the four, a line without `=` and a text over the cap are said, and the rest of
/// the text is still read.
///
/// Mutation: a text read past 4 KB; a bad line that stops the reading.
#[test]
fn what_the_script_cannot_say_is_told_and_the_rest_is_read() {
    let s = parse("colour=blue\nthis is no line\nlocked=1\n");
    assert!(s.locked);
    assert_eq!(s.problems.len(), 2, "{:?}", s.problems);
    assert!(s.problems[0].contains("colour") && s.problems[1].contains("no line"), "{:?}", s.problems);
    let mut big = String::from("locked=1\n");
    big.push_str(&"# padding\n".repeat(600));
    big.push_str("asleep=1\n");
    assert!(big.len() > MAX_BYTES);
    let s = parse(&big);
    assert!(s.locked && !s.display_asleep, "the text is read up to its cap and no further");
    assert!(s.problems.iter().any(|p| p.contains("4096")), "{:?}", s.problems);
}
