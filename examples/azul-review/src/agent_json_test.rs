//! The agents' JSON facade: bounded before parsing, strict, and tolerant only of what a model gets
//! wrong (a raw line break in a string, a comma before a bracket). No window, no process.

use super::*;

const LIMIT: usize = 4_096;

#[test]
fn a_valid_object_is_read_with_its_nesting_numbers_and_escapes() {
    let v = parse(
        r#"{"a": [1, 2.5, -3e2, true, null], "b": {"c": "x\ny\u00e4\ud83d\ude00"}}"#,
        LIMIT,
    )
    .expect("valid");
    assert_eq!(get(&v, "a").and_then(Json::as_array).map(Vec::len), Some(5));
    let c = get(get(&v, "b").expect("b"), "c")
        .and_then(Json::as_str)
        .expect("c");
    assert_eq!(c, "x\nyä😀", "escapes and a surrogate pair are decoded");
}

#[test]
fn text_after_the_value_is_not_json() {
    let e = parse(r#"{"a": 1} and then prose"#, LIMIT).expect_err("trailing text");
    assert!(matches!(e, JsonError::Syntax { line: 1, .. }), "{e:?}");
}

#[test]
fn nothing_but_white_space_is_empty_not_a_syntax_error() {
    assert_eq!(parse("  \n\t ", LIMIT), Err(JsonError::Empty));
    assert_eq!(parse("", LIMIT), Err(JsonError::Empty));
}

#[test]
fn a_text_over_the_limit_is_refused_before_it_is_parsed() {
    let big = format!("[{}]", "1,".repeat(3_000) + "1");
    assert!(big.len() > LIMIT);
    assert_eq!(
        parse(&big, LIMIT),
        Err(JsonError::TooLarge {
            size: big.len(),
            limit: LIMIT
        })
    );
    // the same text is fine when the limit allows it
    assert!(parse(&big, big.len()).is_ok());
}

#[test]
fn the_error_names_the_line_and_the_column() {
    let e = parse("{\n  \"a\": 1,\n  \"b\": oops\n}", LIMIT).expect_err("oops is no value");
    match e {
        JsonError::Syntax { line, column, what } => {
            assert_eq!(line, 3);
            assert!(column >= 8, "column {column}");
            assert!(!what.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn nesting_deeper_than_the_parser_allows_is_an_error_not_a_stack_overflow() {
    let deep = format!("{}{}", "[".repeat(5_000), "]".repeat(5_000));
    let e = parse(&deep, 1 << 20).expect_err("too deep");
    assert!(matches!(e, JsonError::Syntax { .. }), "{e:?}");
}

#[test]
fn a_raw_line_break_inside_a_string_is_repaired_and_said() {
    let (v, repaired) =
        parse_lenient("{\"lines\": [\"a\nb\", \"c\td\"]}", LIMIT).expect("repairable");
    assert!(repaired);
    let lines = array_of(&v, "lines").expect("lines");
    assert_eq!(lines[0].as_str(), Some("a\nb"));
    assert_eq!(lines[1].as_str(), Some("c\td"));
}

#[test]
fn a_comma_before_a_closing_bracket_is_repaired_and_said() {
    let (v, repaired) =
        parse_lenient(r#"{"a": [1, 2, ], "b": {"c": 3,},}"#, LIMIT).expect("repairable");
    assert!(repaired);
    assert_eq!(array_of(&v, "a").map(<[Json]>::len), Some(2));
    assert_eq!(u64_of(get(&v, "b").expect("b"), "c"), Some(3));
}

#[test]
fn valid_json_is_read_strictly_and_not_marked_repaired() {
    let (v, repaired) = parse_lenient(r#"{"a": "x, ]"}"#, LIMIT).expect("valid");
    assert!(!repaired);
    assert_eq!(str_of(&v, "a"), Some("x, ]"));
}

#[test]
fn what_is_not_repairable_stays_an_error_with_the_strict_position() {
    let e = parse_lenient("{\"a\": }", LIMIT).expect_err("no value");
    assert!(matches!(e, JsonError::Syntax { line: 1, .. }), "{e:?}");
}

#[test]
fn repair_leaves_commas_and_brackets_inside_strings_alone() {
    let text = r#"{"a": "one, two,]", "b": "}, ]"}"#;
    assert_eq!(repair(text), text);
}

#[test]
fn repair_handles_an_escaped_quote_before_a_raw_line_break() {
    let text = "{\"a\": \"say \\\"hi\\\"\nthere\"}";
    let fixed = repair(text);
    assert_eq!(fixed, "{\"a\": \"say \\\"hi\\\"\\nthere\"}");
    assert_eq!(
        str_of(&parse(&fixed, LIMIT).expect("valid now"), "a"),
        Some("say \"hi\"\nthere")
    );
}

#[test]
fn repair_escapes_other_control_characters_as_unicode() {
    assert_eq!(repair("\"a\u{1}b\""), "\"a\\u0001b\"");
}

#[test]
fn an_object_is_found_inside_a_code_fence() {
    let text =
        "Here you go:\n```json\n{\"answer\": \"ok\", \"suggestions\": []}\n```\nAnything else?";
    let found = find_object(text, LIMIT).expect("small").expect("an object");
    assert_eq!(str_of(&found.value, "answer"), Some("ok"));
    assert!(!found.repaired);
    assert_eq!(
        &text[found.range.clone()],
        "{\"answer\": \"ok\", \"suggestions\": []}"
    );
}

#[test]
fn a_brace_in_prose_is_passed_over() {
    let text = "Use {x} as the name, then: {\"answer\": \"renamed\"} done";
    let found = find_object(text, LIMIT).expect("small").expect("an object");
    assert_eq!(str_of(&found.value, "answer"), Some("renamed"));
}

#[test]
fn the_first_object_wins_and_text_after_it_is_ignored() {
    let text = "{\"n\": 1} {\"n\": 2}";
    let found = find_object(text, LIMIT).expect("small").expect("an object");
    assert_eq!(u64_of(&found.value, "n"), Some(1));
    assert_eq!(found.range, 0..8);
}

#[test]
fn an_object_with_a_raw_line_break_is_found_and_marked_repaired() {
    let text = "```json\n{\"lines\": [\"a\nb\"]}\n```";
    let found = find_object(text, LIMIT).expect("small").expect("an object");
    assert!(found.repaired);
    assert_eq!(array_of(&found.value, "lines").map(<[Json]>::len), Some(1));
}

#[test]
fn no_object_in_the_text_is_none_and_a_big_text_is_refused() {
    assert_eq!(find_object("just prose, no braces", LIMIT), Ok(None));
    assert_eq!(
        find_object("[1, 2, 3]", LIMIT),
        Ok(None),
        "an array is no object"
    );
    let big = "x".repeat(LIMIT + 1);
    assert!(matches!(
        find_object(&big, LIMIT),
        Err(JsonError::TooLarge { .. })
    ));
}

#[test]
fn an_object_the_caller_does_not_want_is_passed_over_with_what_is_inside_it() {
    let text = "example: {\"name\": \"x\", \"inner\": {\"answer\": \"not this\"}} and then {\"answer\": \"this\"}";
    let wants = |v: &Json| v.get("answer").is_some();
    let found = find_object_where(text, LIMIT, &wants)
        .expect("small")
        .expect("an object");
    assert_eq!(str_of(&found.value, "answer"), Some("this"));
    assert_eq!(
        find_object_where("only {\"name\": 1}", LIMIT, &wants),
        Ok(None)
    );
}

#[test]
fn a_text_of_many_open_braces_is_given_up_on_quickly() {
    let text = "{".repeat(200_000);
    assert_eq!(find_object(&text, 1 << 20), Ok(None));
}

#[test]
fn the_accessors_answer_by_type_and_never_panic() {
    let v = parse(
        r#"{"s": "x", "n": 7, "neg": -1, "f": 1.5, "b": true, "a": [1], "z": null}"#,
        LIMIT,
    )
    .expect("valid");
    assert_eq!(str_of(&v, "s"), Some("x"));
    assert_eq!(u64_of(&v, "n"), Some(7));
    assert_eq!(u64_of(&v, "neg"), None, "never negative");
    assert_eq!(u64_of(&v, "f"), None, "not a whole number");
    assert_eq!(f64_of(&v, "f"), Some(1.5));
    assert_eq!(f64_of(&v, "n"), Some(7.0));
    assert_eq!(bool_of(&v, "b"), Some(true));
    assert_eq!(array_of(&v, "a").map(<[Json]>::len), Some(1));
    assert_eq!(str_of(&v, "n"), None);
    assert_eq!(str_of(&v, "missing"), None);
    assert_eq!(str_of(&Json::Null, "s"), None, "not an object");
    assert_eq!(str_of(&json!([1, 2]), "s"), None);
}

#[test]
fn a_quoted_string_reads_back_as_itself() {
    for text in [
        "plain",
        "quote \" and \\ slash",
        "line\nbreak\ttab",
        "ctrl \u{7} bell",
        "ünï😀",
        "",
    ] {
        let quoted = quote(text);
        assert!(quoted.starts_with('"') && quoted.ends_with('"'));
        assert_eq!(parse(&quoted, LIMIT).expect("valid").as_str(), Some(text));
    }
}

#[test]
fn the_compact_text_has_no_white_space_and_reads_back() {
    let v = json!({"a": [1, 2], "b": "x y"});
    let text = to_text(&v);
    assert!(!text.contains("\n") && !text.contains(": "), "{text}");
    assert_eq!(parse(&text, LIMIT).expect("valid"), v);
}
