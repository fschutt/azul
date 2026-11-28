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
