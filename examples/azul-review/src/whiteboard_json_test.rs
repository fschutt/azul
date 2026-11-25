//! The small JSON reader and writer of the whiteboard's file: what it reads, what it refuses, how it
//! writes. No window, no azul.

use super::*;

fn ok(text: &str) -> Json {
    parse(text).unwrap_or_else(|e| panic!("{text:?}: {e:?}"))
}

#[test]
fn it_reads_every_kind_of_value() {
    assert_eq!(ok("null"), Json::Null);
    assert_eq!(ok(" true "), Json::Bool(true));
    assert_eq!(ok("false"), Json::Bool(false));
    assert_eq!(ok("12"), Json::Num(12.0));
    assert_eq!(ok("-0.5e2"), Json::Num(-50.0));
    assert_eq!(ok("\"a\""), Json::Str("a".to_string()));
    assert_eq!(ok("[1, [2], {}]"), Json::Arr(vec![Json::Num(1.0), Json::Arr(vec![Json::Num(2.0)]), Json::Obj(vec![])]));
    assert_eq!(
        ok("{\"a\": 1, \"b\": [true]}"),
        Json::Obj(vec![("a".to_string(), Json::Num(1.0)), ("b".to_string(), Json::Arr(vec![Json::Bool(true)]))])
    );
}

#[test]
fn strings_have_their_escapes_and_their_unicode() {
    assert_eq!(ok(r#""a\"b\\c\/d\n\t\r\b\f""#), Json::Str("a\"b\\c/d\n\t\r\u{8}\u{c}".to_string()));
    assert_eq!(ok(r#""\u00e4\u00DF""#), Json::Str("äß".to_string()));
    assert_eq!(ok(r#""\ud83d\ude00""#), Json::Str("\u{1F600}".to_string()), "a surrogate pair is one character");
    assert_eq!(ok("\"straight ünïcode ✓\""), Json::Str("straight ünïcode ✓".to_string()));
}

#[test]
fn what_is_not_json_is_refused_with_the_place() {
    for bad in [
        "", "{", "[1,", "[1,]", "{\"a\":1,}", "{\"a\" 1}", "{a: 1}", "'a'", "tru", "01", "1.", "-", ".5", "1e", "\"abc", "\"a\nb\"",
        "\"\\x\"", "\"\\u12\"", "\"\\ud83d\"", "\"\\ude00\"", "[1] [2]", "nan", "{\"a\":1}x", "1 2",
    ] {
        assert!(parse(bad).is_err(), "{bad:?} is no JSON");
    }
    let e = parse("[1, 2, oops]").expect_err("refused");
    assert_eq!(e.at, 7, "{e:?}");
}

#[test]
fn a_document_nested_too_deep_or_too_long_is_refused_not_followed() {
    let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
    assert!(parse(&deep).is_err());
    let fine = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
    assert!(parse(&fine).is_ok());
    // A hundred thousand opening brackets must not use the stack up.
    assert!(parse(&"[".repeat(100_000)).is_err());
    let long = format!("\"{}\"", "a".repeat(MAX_TEXT));
    assert_eq!(parse(&long).expect_err("too long").why, "the text is longer than a board file may be");
}

#[test]
fn the_values_are_read_back_by_their_kind_and_a_wrong_kind_is_none() {
    let j = ok("{\"n\": 7, \"x\": 2.5, \"neg\": -1, \"big\": 1e20, \"s\": \"t\", \"b\": true, \"a\": [1]}");
    assert_eq!(j.get("n").and_then(Json::as_u64), Some(7));
    assert_eq!(j.get("x").and_then(Json::as_u64), None, "not a whole number");
    assert_eq!(j.get("neg").and_then(Json::as_u64), None, "negative");
    assert_eq!(j.get("big").and_then(Json::as_u64), None, "too large to be exact");
    assert_eq!(j.get("x").and_then(Json::as_f32), Some(2.5));
    assert_eq!(j.get("s").and_then(Json::as_str), Some("t"));
    assert_eq!(j.get("b").and_then(Json::as_bool), Some(true));
    assert_eq!(j.get("a").and_then(Json::as_arr).map(<[Json]>::len), Some(1));
    assert!(j.get("missing").is_none());
    assert!(j.get("s").and_then(Json::as_f32).is_none());
    assert!(Json::Null.get("a").is_none());
}

#[test]
fn the_first_of_two_equal_keys_is_the_one_read() {
    assert_eq!(ok("{\"a\": 1, \"a\": 2}").get("a").and_then(Json::as_u64), Some(1));
}

#[test]
fn numbers_are_written_as_short_as_they_read_back() {
    assert_eq!(num(0.0), "0");
    assert_eq!(num(3.0), "3");
    assert_eq!(num(-2.5), "-2.5");
    assert_eq!(num(0.1), "0.1");
    assert_eq!(num(1234.5), "1234.5");
    assert_eq!(num(f32::NAN), "0");
    assert_eq!(num(f32::INFINITY), "0");
    assert_eq!(num(-0.0), "0");
    for v in [0.1_f32, 1.0 / 3.0, 123_456.79, 1e-5, 99_999.99] {
        let back = ok(&num(v)).as_f32().expect("a number");
        assert_eq!(back, v, "{v} written as {}", num(v));
    }
}

#[test]
fn strings_are_written_with_what_json_needs_escaped() {
    let mut out = String::new();
    push_string(&mut out, "a\"b\\c\nd\te\u{1}ä");
    assert_eq!(out, "\"a\\\"b\\\\c\\nd\\te\\u0001ä\"");
    assert_eq!(ok(&out), Json::Str("a\"b\\c\nd\te\u{1}ä".to_string()));
}
