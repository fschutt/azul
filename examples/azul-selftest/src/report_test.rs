//! The verdicts and the report line. No device.

use super::*;

fn row(id: &str, verdict: Verdict) -> RowResult {
    RowResult {
        id: id.into(),
        group: "g".into(),
        title: "T".into(),
        verdict,
        ms: 5,
    }
}

fn context() -> Context {
    Context {
        app: "AzSelfTest".into(),
        version: "0.1.0".into(),
        platform: "android".into(),
        arch: "x86_64".into(),
        unattended: true,
        total_ms: 1234,
    }
}

/// The rule of the self-test: only a capability that said it was available and then failed is a failure.
///
/// Mutation: `is_failure` true for `Unavailable` (a desktop running the phone-only probes would exit 1).
#[test]
fn only_a_fail_makes_the_exit_code_non_zero() {
    let quiet = [
        row("a", Verdict::Pass("x".into())),
        row("b", Verdict::Unavailable("no sensor".into())),
        row("c", Verdict::Denied("refused".into())),
        row("d", Verdict::NeedsHuman("shake it".into())),
        row("e", Verdict::Pending("no probe".into())),
    ];
    assert_eq!(exit_code(&quiet), 0);
    let mut loud = quiet.to_vec();
    loud.push(row("f", Verdict::Fail("said available, no reading".into())));
    assert_eq!(exit_code(&loud), 1);
    assert_eq!(exit_code(&[]), 0, "an empty report is not a failure (the harness checks the row count)");
}

#[test]
fn every_verdict_has_its_word_and_its_text() {
    let all = [
        (Verdict::Pass("p".into()), "PASS", "p"),
        (Verdict::Unavailable("u".into()), "UNAVAILABLE", "u"),
        (Verdict::Denied("d".into()), "DENIED", "d"),
        (Verdict::Fail("f".into()), "FAIL", "f"),
        (Verdict::NeedsHuman("h".into()), "NEEDS-HUMAN", "h"),
        (Verdict::Pending("n".into()), "PENDING", "n"),
    ];
    for (verdict, code, detail) in all {
        assert_eq!((verdict.code(), verdict.detail()), (code, detail));
    }
}

#[test]
fn a_json_string_escapes_what_would_break_the_line() {
    assert_eq!(json_string("plain"), "\"plain\"");
    assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    assert_eq!(json_string("one\ntwo\r\tthree"), "\"one\\ntwo\\r\\tthree\"");
    assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
    assert_eq!(json_string("Grüße 日本"), "\"Grüße 日本\"", "text that is not ASCII goes through");
}

/// A harness greps ONE line from a log: the prefix, then JSON with no newline in it, whatever the details hold.
///
/// Mutation: a detail with a newline written as it is (the line breaks in the middle of the JSON).
#[test]
fn the_report_is_one_line_with_the_prefix() {
    let rows = [
        row("sensors.accelerometer", Verdict::Pass("x=0.1\ny=9.8".into())),
        row("security.keyring", Verdict::Fail("store said \"Error\"".into())),
    ];
    let line = report_line(&context(), &rows);
    assert!(line.starts_with("AZ_SELFTEST_REPORT {"), "{line}");
    assert!(!line.contains('\n') && !line.contains('\r'), "{line}");
    assert!(line.ends_with("]}"), "{line}");
    assert!(line.contains("\"platform\":\"android\""));
    assert!(line.contains("\"unattended\":true"));
    assert!(line.contains("\"pass\":1,\"unavailable\":0,\"denied\":0,\"fail\":1,\"needs_human\":0,\"pending\":0"));
    assert!(line.contains("{\"id\":\"sensors.accelerometer\",\"group\":\"g\",\"title\":\"T\",\"verdict\":\"PASS\",\"detail\":\"x=0.1\\ny=9.8\",\"ms\":5}"));
    assert!(line.contains("\"detail\":\"store said \\\"Error\\\"\""));
}

#[test]
fn the_counts_add_up_to_the_rows() {
    let rows = [
        row("a", Verdict::Pass(String::new())),
        row("b", Verdict::Pass(String::new())),
        row("c", Verdict::Denied(String::new())),
        row("d", Verdict::Pending(String::new())),
    ];
    let c = counts(&rows);
    assert_eq!((c.pass, c.unavailable, c.denied, c.fail, c.needs_human, c.pending), (2, 0, 1, 0, 0, 1));
}

/// The row list of a harness: ids split at commas, blanks and spaces dropped.
#[test]
fn a_row_list_is_ids_split_at_commas() {
    assert_eq!(parse_row_list("sensors.geolocation"), vec!["sensors.geolocation".to_string()]);
    assert_eq!(
        parse_row_list(" system.notifications , ,sensors.geolocation,"),
        vec!["system.notifications".to_string(), "sensors.geolocation".to_string()]
    );
    assert!(parse_row_list("").is_empty());
    assert!(parse_row_list(" , ").is_empty());
}

/// The rows of a long report: enough of them, with long details, that one line cannot hold the JSON.
fn long_rows(count: usize, detail: &str) -> Vec<RowResult> {
    (0..count).map(|n| row(&format!("group.row_{n:03}"), Verdict::Pass(detail.to_string()))).collect()
}

/// The unwrap of the parts: the slices behind the headers `AZ_SELFTEST_REPORT_PART i/n `, checked to be 1..=n in order.
fn joined_parts(lines: &[String]) -> String {
    let mut json = String::new();
    for (at, line) in lines.iter().enumerate() {
        let header = format!("{PART_PREFIX} {}/{} ", at + 1, lines.len());
        assert!(line.starts_with(&header), "{line}");
        json.push_str(&line[header.len()..]);
    }
    json
}

/// A report that fits is the one line the harnesses already read.
#[test]
fn a_short_report_stays_one_line() {
    let rows = [row("a.b", Verdict::Pass("ok".into()))];
    assert_eq!(report_lines(&context(), &rows, MAX_LINE_BYTES), vec![report_line(&context(), &rows)]);
}

/// Logcat keeps about 4000 bytes of a line and cuts the rest without a word: the report of 40 rows has to leave in lines that are each
/// short enough, and the slices must give back the JSON exactly.
///
/// Mutation: lines of the whole report (what the first emulator run printed: it ended in the 17th row).
#[test]
fn a_long_report_leaves_in_numbered_parts_that_join_to_the_json() {
    let rows = long_rows(40, "x=0.1 y=9.8 z=0.3 via the platform's sensor hub, 3 samples in 140 ms");
    let json = report_json(&context(), &rows);
    assert!(json.len() > 2 * MAX_LINE_BYTES, "the fixture must be longer than two lines ({})", json.len());
    let lines = report_lines(&context(), &rows, MAX_LINE_BYTES);
    assert!(lines.len() >= 3, "{}", lines.len());
    for line in &lines {
        assert!(line.len() <= MAX_LINE_BYTES, "{} bytes: {line}", line.len());
        assert!(!line.contains('\n') && !line.contains('\r'));
    }
    assert_eq!(joined_parts(&lines), json);
}

/// A slice ends between two characters (a Rust string cannot be cut anywhere else) and a multi-byte character is not lost at a seam.
#[test]
fn parts_cut_between_characters_and_lose_none() {
    let rows = long_rows(60, "Grüße aus Tübingen 日本語のテキスト 🎉 and more");
    let json = report_json(&context(), &rows);
    for max in [300, 301, 302, 303, 1000, 3000] {
        let lines = report_lines(&context(), &rows, max);
        assert!(lines.iter().all(|l| l.len() <= max), "max {max}");
        assert_eq!(joined_parts(&lines), json, "max {max}");
    }
}

/// A log may drop the blanks at the end of a line, and the slices have to join to the JSON: no slice ends in a blank.
///
/// Mutation: cutting at the byte limit whatever stands there.
#[test]
fn no_part_ends_in_a_blank() {
    let rows = long_rows(30, "a   b    c     d      e       f        g         h          i           j");
    let json = report_json(&context(), &rows);
    for max in [200, 257, 400, 999, 3000] {
        let lines = report_lines(&context(), &rows, max);
        for line in &lines {
            assert!(!line.ends_with(char::is_whitespace), "max {max}: {line:?}");
        }
        assert_eq!(joined_parts(&lines), json, "max {max}");
    }
    // nothing but blanks in a whole slice: cut at the limit, still lose nothing
    let blanks = long_rows(2, &" ".repeat(900));
    let lines = report_lines(&context(), &blanks, 400);
    assert_eq!(joined_parts(&lines), report_json(&context(), &blanks));
}
