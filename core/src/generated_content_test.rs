//! What `content: counter(page)` says: the page counters of paged media are not
//! counters of the document, so they render 0 - and the engine tells the one
//! who wrote them.

use super::*;

/// What every report of a page counter says.
const FINDING: &str = "in `content` renders 0";

/// Whether the engine reported `function(counter)`: the report starts with the
/// function and the counter it is about. The bare name proves nothing - the hint
/// every report ends with names `counter(page)`, so a report of `counter(pages)`
/// holds it too.
fn reported(function: &str, counter: &str) -> bool {
    crate::diagnostics::any_contains(&alloc::format!(
        "[azul][content] {function}({counter}) {FINDING}"
    ))
}

/// The text of `value` with `counters` in scope.
fn text(value: &str, counters: &Counters) -> String {
    content_text(value, None, counters, &[], &mut 0)
}

/// Holds the shared diagnostics ring for one test and starts it empty.
fn quiet_diagnostics() -> std::sync::MutexGuard<'static, ()> {
    let lock = crate::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    crate::diagnostics::clear();
    lock
}

/// The silent zero this replaces: "Page 0 of 0" on every page, and nothing said.
#[test]
fn page_and_pages_render_zero_and_say_so() {
    let _lock = quiet_diagnostics();

    let said = text(
        "\"Page \" counter(page) \" of \" counter(pages)",
        &Counters::default(),
    );

    assert_eq!(said, "Page 0 of 0", "the text is as it was");
    assert!(
        reported("counter", "page"),
        "{:?}",
        crate::diagnostics::recorded()
    );
    assert!(
        reported("counter", "pages"),
        "{:?}",
        crate::diagnostics::recorded()
    );
}

/// Each counter is reported for itself: a text that holds `counter(pages)` alone
/// does not report `counter(page)`, though the hint of its report names it.
#[test]
fn each_page_counter_is_reported_for_itself() {
    let _lock = quiet_diagnostics();

    assert_eq!(text("counter(pages)", &Counters::default()), "0");

    assert!(reported("counter", "pages"), "{:?}", crate::diagnostics::recorded());
    assert!(
        !reported("counter", "page"),
        "counter(page) was not asked for: {:?}",
        crate::diagnostics::recorded()
    );
}

/// `counters(page, ...)` reads the same missing counter.
#[test]
fn counters_of_page_say_so_too() {
    let _lock = quiet_diagnostics();

    assert_eq!(text("counters(page, \".\")", &Counters::default()), "0");

    assert!(reported("counters", "page"), "{:?}", crate::diagnostics::recorded());
}

/// A document that counts something of its own and calls it `page` has the
/// counter: nothing is missing, nothing is reported.
#[test]
fn a_counter_the_document_made_is_not_reported() {
    let _lock = quiet_diagnostics();
    let mut counters = Counters::default();
    counters.reset("page", 7, 0, None);

    assert_eq!(text("counter(page)", &counters), "7");

    assert!(!crate::diagnostics::any_contains(FINDING));
}

/// Other counters, defined or not, are the document's business.
#[test]
fn other_counters_are_not_reported() {
    let _lock = quiet_diagnostics();

    assert_eq!(text("counter(chapter)", &Counters::default()), "0");

    assert!(!crate::diagnostics::any_contains(FINDING));
}

/// A finding is said once per process, not once per element that holds it.
#[test]
fn the_finding_is_said_once() {
    let _lock = quiet_diagnostics();

    for _ in 0..3 {
        drop(text("counter(page)", &Counters::default()));
    }

    let said = crate::diagnostics::recorded()
        .iter()
        .filter(|line| line.contains("[azul][content] counter(page) in"))
        .count();
    assert_eq!(said, 1);
}
