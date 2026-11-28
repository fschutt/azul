use super::*;

fn rust() -> &'static SyntaxReference {
    syntax_for("main.rs", "")
}

fn source(text: &str) -> impl Fn(usize) -> String {
    let lines: Vec<String> = text.split('\n').map(String::from).collect();
    move |i| lines.get(i).cloned().unwrap_or_default()
}

fn kind_at(spans: &[CodeSpan], byte: u32) -> TokenKind {
    spans
        .iter()
        .find(|s| s.start <= byte && byte < s.end)
        .map_or(TokenKind::Plain, |s| s.kind)
}

#[test]
fn a_rust_line_has_keyword_function_and_string_runs() {
    let mut h = Highlighter::new(rust());
    assert_eq!(h.language(), "Rust");
    let text = "fn main() { let s = \"hi\"; }";
    let spans = h.line_spans(0, &source(text)).expect("line 0 is near");
    assert_eq!(kind_at(&spans, 0), TokenKind::Keyword, "{spans:?}");
    assert_eq!(kind_at(&spans, 4), TokenKind::Function, "{spans:?}");
    let quote = text.find('"').expect("a string") as u32;
    assert_eq!(kind_at(&spans, quote + 1), TokenKind::StringLiteral, "{spans:?}");
    assert!(spans.windows(2).all(|w| w[0].end <= w[1].start), "sorted, apart");
}

#[test]
fn a_block_comment_carries_its_state_to_the_next_lines() {
    let mut h = Highlighter::new(rust());
    let src = source("/* start\nstill comment\nend */ fn x() {}");
    let middle = h.line_spans(1, &src).expect("near");
    assert_eq!(kind_at(&middle, 0), TokenKind::Comment);
    assert_eq!(kind_at(&middle, 8), TokenKind::Comment);
    let last = h.line_spans(2, &src).expect("near");
    assert_eq!(kind_at(&last, 0), TokenKind::Comment);
    assert_eq!(kind_at(&last, 7), TokenKind::Keyword, "after the comment: fn");
}

#[test]
fn an_edit_invalidates_from_its_line_and_the_lines_above_stay_cached() {
    let text: String = (0..50).map(|i| format!("let x{i} = {i};")).collect::<Vec<_>>().join("\n");
    let mut h = Highlighter::new(rust());
    h.line_spans(49, &source(&text)).expect("near");
    assert!(h.is_cached(19) && h.is_cached(20) && h.is_cached(49));
    h.edited(20, 0, 0);
    assert!(h.is_cached(19), "a line above the edit keeps its colours");
    assert!(!h.is_cached(20) && !h.is_cached(49));
}

#[test]
fn opening_a_comment_recolours_the_lines_after_it() {
    let before = "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}";
    let mut h = Highlighter::new(rust());
    let spans = h.line_spans(3, &source(before)).expect("near");
    assert_eq!(kind_at(&spans, 0), TokenKind::Keyword);
    let after = "fn a() {}\nfn b() {}\n/* fn c() {}\nfn d() {}";
    h.edited(2, 0, 0);
    let spans = h.line_spans(3, &source(after)).expect("near");
    assert_eq!(kind_at(&spans, 0), TokenKind::Comment, "line 3 is inside the comment now");
}

#[test]
fn a_far_line_waits_for_the_background_walk_then_comes_from_its_checkpoints() {
    let text: String = (0..6000).map(|i| format!("fn f{i}() {{}}")).collect::<Vec<_>>().join("\n");
    let src = source(&text);
    let mut h = Highlighter::new(rust());
    assert!(h.line_spans(5000, &src).is_none(), "5000 lines are too many for the UI thread");
    let job = h.job_for(5000, &src);
    let result = job.run();
    assert!(h.adopt(result));
    h.lines_parsed = 0;
    let spans = h.line_spans(5000, &src).expect("from the nearest checkpoint");
    assert_eq!(kind_at(&spans, 0), TokenKind::Keyword);
    assert!(h.lines_parsed <= EVERY, "{} lines parsed", h.lines_parsed);
    // A job started before an edit is not taken.
    let stale = h.job_for(5900, &src);
    h.edited(10, 0, 0);
    assert!(!h.adopt(stale.run()));
}

#[test]
fn after_an_edit_that_keeps_the_state_the_old_checkpoints_are_taken_back() {
    let text: String = (0..400).map(|i| format!("let x{i} = {i};")).collect::<Vec<_>>().join("\n");
    let src = source(&text);
    let mut h = Highlighter::new(rust());
    h.line_spans(300, &src).expect("near");
    h.cache.clear();
    h.edited(10, 0, 0);
    h.lines_parsed = 0;
    h.line_spans(300, &src).expect("near");
    assert!(
        h.lines_parsed < 2 * EVERY,
        "{} lines parsed: the walk met an old checkpoint and jumped",
        h.lines_parsed
    );
}
