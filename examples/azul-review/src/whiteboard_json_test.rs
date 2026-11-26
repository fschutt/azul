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

#[test]
fn a_document_is_written_for_people_and_reads_back_to_what_it_was() {
    let v = object(vec![
        ("format", from_str("azboard/1")),
        ("short", Json::Arr(vec![from_u64(1), from_u64(2), Json::Null, Json::Bool(true)])),
        ("point", object(vec![("x", from_f32(0.1)), ("y", from_f32(-2.5))])),
        (
            "items",
            Json::Arr(vec![
                object(vec![("id", from_u64(1)), ("label", from_str("a \"quoted\"\nlabel")), ("lines", Json::Arr(vec![from_str("fn main() {"), from_str("}")]))]),
                object(vec![("id", from_u64(2)), ("nested", object(vec![("deep", Json::Arr(vec![object(vec![("k", from_u64(3))])]))]))]),
            ]),
        ),
        ("empty_list", Json::Arr(vec![])),
        ("empty_object", Json::Obj(vec![])),
        ("long", Json::Arr((0..60).map(from_u64).collect())),
    ]);
    let text = pretty(&v);
    assert_eq!(parse(&text), Ok(v), "what is written is what is read back");
    assert!(text.ends_with("}\n"), "a line feed at the end");
    // Short lists and objects of scalars are one line; others take a line for each.
    assert!(text.contains("\"short\": [1, 2, null, true]"), "{text}");
    assert!(text.contains("\"point\": {\"x\": 0.1, \"y\": -2.5}"), "{text}");
    assert!(text.contains("\"empty_list\": []") && text.contains("\"empty_object\": {}"), "{text}");
    assert!(text.contains("  \"items\": [\n    {"), "{text}");
    assert!(text.lines().all(|l| l.len() < 130), "no line of a short document is long: {text}");
    // The list of 60 numbers is too long for a line: one number to a line.
    assert!(text.contains("\"long\": [\n    0,\n    1,"), "{text}");
}

#[test]
fn a_number_made_of_an_f32_keeps_the_digits_of_the_f32() {
    let text = pretty(&Json::Arr(vec![from_f32(0.1), from_f32(0.7), from_f32(-120.5), from_f32(1.0 / 3.0), from_f32(0.0), from_f32(f32::NAN), from_f32(100_000.0)]));
    assert_eq!(text, "[0.1, 0.7, -120.5, 0.33333334, 0, 0, 100000]\n");
    // And back to the very same f32.
    for v in [0.1_f32, 0.7, -120.5, 1.0 / 3.0, 12_345.678, 1e-5] {
        assert_eq!(parse(&pretty(&from_f32(v))).ok().and_then(|j| j.as_f32()), Some(v), "{v}");
    }
    assert_eq!(from_u64(u64::MAX), Json::Num(9_007_199_254_740_992.0), "an id past what a double holds exactly is held at the limit");
}

#[test]
fn a_longer_text_is_read_when_the_caller_allows_it_and_not_otherwise() {
    let text = format!("[\"{}\"]", "a".repeat(MAX_TEXT));
    assert!(parse(&text).is_err(), "the manifest's limit");
    assert_eq!(parse_with(&text, MAX_TEXT + 10).ok().and_then(|j| j.as_arr().map(<[_]>::len)), Some(1));
    assert!(parse_with(&text, 100).is_err());
}

/// (the bits of a double, what ECMAScript prints for it): the numbers of RFC 8785's appendix B and some of the board's,
/// printed by V8.
const ES_NUMBERS: &[(u64, &str)] = &[
    (0x0000_0000_0000_0000, "0"),
    (0x8000_0000_0000_0000, "0"),
    (0x0000_0000_0000_0001, "5e-324"),
    (0x8000_0000_0000_0001, "-5e-324"),
    (0x7fef_ffff_ffff_ffff, "1.7976931348623157e+308"),
    (0xffef_ffff_ffff_ffff, "-1.7976931348623157e+308"),
    (0x4340_0000_0000_0000, "9007199254740992"),
    (0xc340_0000_0000_0000, "-9007199254740992"),
    (0x4430_0000_0000_0000, "295147905179352830000"),
    (0x44b5_2d02_c7e1_4af5, "9.999999999999997e+22"),
    (0x44b5_2d02_c7e1_4af6, "1e+23"),
    (0x44b5_2d02_c7e1_4af7, "1.0000000000000001e+23"),
    (0x444b_1ae4_d6e2_ef4e, "999999999999999700000"),
    (0x444b_1ae4_d6e2_ef4f, "999999999999999900000"),
    (0x444b_1ae4_d6e2_ef50, "1e+21"),
    (0x3eb0_c6f7_a0b5_ed8c, "9.999999999999997e-7"),
    (0x3eb0_c6f7_a0b5_ed8d, "0.000001"),
    (0x41b3_de43_5555_5553, "333333333.3333332"),
    (0x41b3_de43_5555_5554, "333333333.33333325"),
    (0x41b3_de43_5555_5555, "333333333.3333333"),
    (0x41b3_de43_5555_5556, "333333333.3333334"),
    (0x41b3_de43_5555_5557, "333333333.33333343"),
    (0xbecb_f647_612f_3696, "-0.0000033333333333333333"),
    (0x4314_3ff3_c1cb_0959, "1424953923781206.2"),
    (0x3fb9_9999_9999_999a, "0.1"),
    (0x4059_0000_0000_0000, "100"),
    (0x40c3_8800_0000_0000, "10000"),
    (0x3ee4_f8b5_88e3_68f1, "0.00001"),
    (0x3ff8_0000_0000_0000, "1.5"),
    (0xc008_0000_0000_0000, "-3"),
    (0x42c0_0000_0000_0000, "35184372088832"),
];

#[test]
fn numbers_are_written_as_ecmascript_writes_them() {
    for (bits, want) in ES_NUMBERS {
        let n = f64::from_bits(*bits);
        assert_eq!(es_number(n), *want, "{bits:#018x}");
        assert_eq!(canonical(&Json::Num(n)), *want);
    }
    assert_eq!(es_number(f64::NAN), "0");
    assert_eq!(es_number(f64::INFINITY), "0");
}

#[test]
fn a_document_has_one_canonical_text_the_same_as_any_jcs_library_makes() {
    // The text and what V8's JSON.stringify makes of it with the members sorted (RFC 8785's recipe).
    let text = r#"{"b": [3, 1.0, 2.50, 1e2, -0.0, 1E-7, 123456789012345680000], "a": {"z": null, "y": true, "x": false},
        "\ue000": 1, "\ud83d\ude00": 2, "é": 3, "A": 4, "\u20ac": 5,
        "s": "tab\there \"q\" back\\slash \u0001 \u0008 \u000c \n \r \/ \u00e9 \ud83d\ude00 \u007f", "empty": {}, "list": []}"#;
    let want = "{\"A\":4,\"a\":{\"x\":false,\"y\":true,\"z\":null},\"b\":[3,1,2.5,100,0,1e-7,123456789012345680000],\"empty\":{},\"list\":[],\"s\":\"tab\\there \\\"q\\\" back\\\\slash \\u0001 \\b \\f \\n \\r / \u{e9} \u{1f600} \u{7f}\",\"\u{e9}\":3,\"\u{20ac}\":5,\"\u{1f600}\":2,\"\u{e000}\":1}";
    let v = ok(text);
    assert_eq!(canonical(&v), want);
    // Spaces, the order of the members and how a number was spelled make no difference.
    let other = r#"{"list":[],"empty":{},"\ue000":1,"\ud83d\ude00":2,"\u20ac":5,"é":3,"A":4,"a":{"x":false,"z":null,"y":true},
        "s":"tab\there \"q\" back\\slash \u0001 \b \f \n \r / é \ud83d\ude00 \u007f","b":[3,1,2.5,1E2,0,0.0000001,123456789012345680000]}"#;
    assert_eq!(canonical(&ok(other)), want);
    // And it is JSON: what is read from it is the same value.
    assert_eq!(ok(&canonical(&v)), ok(&canonical(&ok(other))));
}

#[test]
fn the_text_a_board_is_written_as_hashes_the_same_when_it_is_read_back_and_written_again() {
    let v = object(vec![("b", from_f32(0.1)), ("a", Json::Arr(vec![from_f32(-120.5), from_u64(7), from_f32(1.0 / 3.0)])), ("t", from_str("x\ny"))]);
    let once = canonical(&v);
    assert_eq!(once, "{\"a\":[-120.5,7,0.33333334],\"b\":0.1,\"t\":\"x\\ny\"}");
    assert_eq!(canonical(&ok(&pretty(&v))), once, "through the readable text and back");
}

