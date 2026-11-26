//! The startup timeline: what it parses of the OS (pure), what it records and what it prints.

use std::{cell::RefCell, time::Instant};

use super::*;

thread_local! {
    /// What the test sink saw on this thread.
    static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn collect(line: &str) {
    LINES.with(|l| l.borrow_mut().push(line.to_owned()));
}

fn taken() -> Vec<String> {
    LINES.with(|l| core::mem::take(&mut *l.borrow_mut()))
}

fn timeline_with_sink() -> StartupTimeline {
    taken();
    StartupTimeline::new(true, Instant::now(), None).with_sink(collect)
}

/// A real `/proc/<pid>/stat` line whose command holds spaces and parentheses: the fields count
/// from the LAST `)`, so field 22 (`starttime`) is the 20th after it.
///
/// Mutation: counting from the first `)`; counting fields from the start of the line; an off by
/// one in the index.
#[test]
fn the_start_ticks_are_field_22_after_the_last_parenthesis_of_proc_stat() {
    let stat = "12345 (my (weird) app) S 1 12345 12345 0 -1 4194560 1000 0 0 0 5 3 0 0 20 0 8 0 987654 123456789 5000 18446744073709551615 1 1 0";
    assert_eq!(proc_stat_start_ticks(stat), Some(987_654));
    assert_eq!(proc_stat_start_ticks("1 (init) S 0 1 1 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 42 1 1"), Some(42));
}

/// A line that is cut short, or has no parenthesis, has no answer instead of a wrong one.
///
/// Mutation: an unwrap on the index; an empty string answered with 0.
#[test]
fn a_cut_proc_stat_line_has_no_start_ticks() {
    assert_eq!(proc_stat_start_ticks(""), None);
    assert_eq!(proc_stat_start_ticks("12345 no parenthesis S 1"), None);
    assert_eq!(proc_stat_start_ticks("12345 (short) S 1 2 3"), None);
    assert_eq!(proc_stat_start_ticks("12345 (bad) S 1 12345 12345 0 -1 4194560 1000 0 0 0 5 3 0 0 20 0 8 0 notanumber 1"), None);
}

/// `/proc/uptime` reads in microseconds, with one to six decimals or none.
///
/// Mutation: the decimals read as milliseconds; the fraction dropped; a second number used.
#[test]
fn the_uptime_is_the_first_number_of_proc_uptime_in_microseconds() {
    assert_eq!(proc_uptime_us("12345.67 98765.43\n"), Some(12_345_670_000));
    assert_eq!(proc_uptime_us("5.5 1.0"), Some(5_500_000));
    assert_eq!(proc_uptime_us("7"), Some(7_000_000));
    assert_eq!(proc_uptime_us("0.000001 0"), Some(1));
    assert_eq!(proc_uptime_us("0.1234567 0"), Some(123_456), "more than six decimals are cut");
    assert_eq!(proc_uptime_us(""), None);
    assert_eq!(proc_uptime_us("abc 1"), None);
    assert_eq!(proc_uptime_us("1.x 1"), None);
}

/// The age is the uptime minus the start: 1000 ticks at 100 per second started at 10 s; read at
/// 10.25 s that is 250 ms. A uptime that is smaller (the 10 ms tick rounds the start up) is no age,
/// not a wrapped one; a tick rate of 0 has no answer.
///
/// Mutation: ticks and seconds mixed up; a wrapping subtraction; the rate not divided.
#[test]
fn the_age_of_a_process_is_the_uptime_minus_its_start_ticks() {
    assert_eq!(age_from_proc_us(1_000, 100, 10_250_000), Some(250_000));
    assert_eq!(age_from_proc_us(1_000, 100, 10_000_000), Some(0));
    assert_eq!(age_from_proc_us(1_000, 100, 9_990_000), Some(0), "never negative");
    assert_eq!(age_from_proc_us(1_000, 0, 10_250_000), None);
    assert_eq!(age_from_proc_us(u64::MAX, 100, 10), None, "an overflow is no answer");
}

fn kinfo(sec: i64, usec: i32) -> Vec<u8> {
    let mut bytes = vec![0u8; 648];
    bytes[0..8].copy_from_slice(&sec.to_ne_bytes());
    bytes[8..12].copy_from_slice(&usec.to_ne_bytes());
    bytes
}

/// macOS and iOS: the first 12 bytes of a `kinfo_proc` are the `timeval` of the process start.
///
/// Mutation: the microseconds read from the wrong offset; seconds taken as microseconds; the
/// microseconds left out.
#[test]
fn the_start_of_a_kinfo_proc_is_its_leading_timeval() {
    assert_eq!(kinfo_proc_start_unix_us(&kinfo(1_764_000_000, 123_456)), Some(1_764_000_000_123_456));
    assert_eq!(kinfo_proc_start_unix_us(&kinfo(1, 0)), Some(1_000_000));
}

/// A buffer that is too short, a zero start (the kernel had nothing) and a microsecond field out of
/// range answer None.
///
/// Mutation: a short buffer sliced anyway; a zero start accepted; `usec >= 1_000_000` accepted.
#[test]
fn a_kinfo_proc_without_a_sane_start_has_none() {
    assert_eq!(kinfo_proc_start_unix_us(&[0u8; 8]), None);
    assert_eq!(kinfo_proc_start_unix_us(&kinfo(0, 5)), None);
    assert_eq!(kinfo_proc_start_unix_us(&kinfo(5, 1_000_000)), None);
    assert_eq!(kinfo_proc_start_unix_us(&kinfo(5, -1)), None);
    assert_eq!(kinfo_proc_start_unix_us(&kinfo(-5, 1)), None);
}

/// Windows: a FILETIME counts 100 ns ticks from 1601; the Unix epoch is 116444736000000000 of them.
///
/// Mutation: ticks taken as microseconds; the epoch offset of 1900; a time before 1970 wrapped.
#[test]
fn a_filetime_becomes_unix_microseconds() {
    let epoch: u64 = 116_444_736_000_000_000;
    let split = |t: u64| (t as u32, (t >> 32) as u32);
    let (lo, hi) = split(epoch);
    assert_eq!(filetime_to_unix_us(lo, hi), Some(0));
    let (lo, hi) = split(epoch + 864_000_000_000);
    assert_eq!(filetime_to_unix_us(lo, hi), Some(86_400_000_000), "one day later");
    let (lo, hi) = split(epoch + 1_234_560);
    assert_eq!(filetime_to_unix_us(lo, hi), Some(123_456));
    assert_eq!(filetime_to_unix_us(0, 0), None, "before 1970 has no Unix time");
}

/// The reader of the real OS runs where there is one, and its answer is sane: the process is
/// younger than a day and the age fits what has run so far. (Skipped where the platform does not
/// say.)
#[test]
fn this_process_has_a_start_time_where_the_os_gives_one() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_micros()).unwrap_or(0))
        .unwrap_or(0);
    let known = cfg!(any(target_os = "macos", target_os = "ios", target_os = "linux", target_os = "android", target_os = "windows"));
    match process_age_at(now) {
        Some(age) => {
            assert!(known, "a platform that is not listed answered");
            assert!(age.micros < 86_400_000_000, "a test process older than a day: {age:?}");
            assert!(!age.source.is_empty());
            assert!(age.resolution_us >= 1);
        }
        None => assert!(!known, "a platform with a source gave none"),
    }
}

/// OFF costs nothing and records nothing: no mark, no detail built, no layout counted, no table.
///
/// Mutation: `mark` records when off; the detail closure runs when off; the layout counter goes up.
#[test]
fn a_timeline_that_is_off_records_nothing_and_builds_nothing() {
    let t = StartupTimeline::disabled().with_sink(collect);
    taken();
    t.mark("a");
    let mut built = 0;
    t.mark_with("b", || {
        built += 1;
        String::from("x")
    });
    assert!(!t.mark_once("c"));
    assert!(!t.mark_once_with("d", || {
        built += 1;
        String::new()
    }));
    assert_eq!(t.layout_begin("Initial"), 0);
    t.layout_end(0, 1, "");
    assert!(t.is_empty());
    assert_eq!(t.layout_count(), 0);
    assert_eq!(built, 0, "the detail is built only for a timeline that is on");
    assert!(!t.print_table());
    assert!(taken().is_empty());
    assert!(!t.enabled());
    // `new` with `enabled = false` is the same thing.
    assert!(!StartupTimeline::new(false, Instant::now(), None).enabled());
}

/// Marks keep the order they were made in, with a clock that does not go back, and say which thread.
///
/// Mutation: marks sorted by name; the clock read outside the lock.
#[test]
fn marks_are_in_the_order_they_were_made_with_a_clock_that_does_not_go_back() {
    let t = timeline_with_sink();
    t.mark("one");
    std::thread::sleep(std::time::Duration::from_millis(3));
    t.mark_with("two", || "detail".to_owned());
    t.mark("three");
    let marks = t.marks();
    let names: Vec<_> = marks.iter().map(|m| m.name).collect();
    assert_eq!(names, ["one", "two", "three"]);
    assert!(marks.windows(2).all(|w| w[0].at_us <= w[1].at_us));
    assert!(marks[1].at_us >= marks[0].at_us + 2_000, "the sleep shows");
    assert_eq!(marks[1].detail, "detail");
    assert!(t.has("two") && !t.has("four"));
    assert!(t.at_us_of("three").is_some());
}

/// The first present, the first event and the first a11y tree are marked ONCE, by whoever is first,
/// from any thread.
///
/// Mutation: a second mark of the same name; `mark_once` true twice; two threads both winning.
#[test]
fn a_once_mark_is_made_once_even_from_two_threads() {
    let t = std::sync::Arc::new(StartupTimeline::new(true, Instant::now(), None));
    assert!(t.mark_once("first_present"));
    assert!(!t.mark_once("first_present"));
    assert!(!t.mark_once_with("first_present", || "never built".to_owned()));
    let wins = std::sync::atomic::AtomicU32::new(0);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                if t.mark_once("race") {
                    wins.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            });
        }
    });
    assert_eq!(wins.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(t.marks().iter().filter(|m| m.name == "race").count(), 1);
}

/// Each layout pass takes the next number and says why it ran; its end carries the number, the
/// lifecycle passes and the phases.
///
/// Mutation: the counter not incremented; the reason dropped; the number of the end different.
#[test]
fn every_layout_pass_is_counted_and_says_why_it_ran() {
    let t = timeline_with_sink();
    let first = t.layout_begin("Initial");
    t.layout_end(first, 1, "after_callback 5.0ms");
    let second = t.layout_begin("Resize");
    t.layout_end(second, 2, "");
    assert_eq!((first, second), (1, 2));
    assert_eq!(t.layout_count(), 2);
    let marks = t.marks();
    assert_eq!(marks[0].name, "layout_begin");
    assert_eq!(marks[0].detail, "#1 reason=Initial");
    assert_eq!(marks[1].detail, "#1 passes=1 after_callback 5.0ms");
    assert_eq!(marks[2].detail, "#2 reason=Resize");
    assert_eq!(marks[3].detail, "#2 passes=2");
}

/// The table prints once, at the first present; what is marked after it prints as one line each, with
/// its time and the thread; nothing prints before.
///
/// Mutation: the table printed twice; a late mark that prints nothing; a mark before the table that
/// prints.
#[test]
fn the_table_prints_once_and_later_marks_print_as_single_lines() {
    let t = timeline_with_sink();
    t.mark("early");
    assert!(taken().is_empty(), "nothing is printed before the first present");
    t.mark_once("first_present");
    assert!(t.print_table());
    let table = taken();
    assert!(table.len() >= 4, "{table:?}");
    assert!(table.iter().all(|l| l.starts_with("[startup] ")), "{table:?}");
    assert!(table.iter().any(|l| l.contains("early")));
    assert!(table.iter().any(|l| l.contains("first_present")));
    assert!(!t.print_table(), "the table is printed once");
    assert!(taken().is_empty());
    assert!(t.table_printed());
    t.mark_with("font_scan_complete", || "fonts=768".to_owned());
    let late = taken();
    assert_eq!(late.len(), 1, "{late:?}");
    assert!(late[0].starts_with("[startup] late: font_scan_complete at "), "{late:?}");
    assert!(late[0].contains("fonts=768"), "{late:?}");
}

/// The step column counts from the previous mark of the SAME thread: a worker's mark between two
/// main-thread marks does not shorten the main thread's step.
///
/// Mutation: the step taken from the previous mark of any thread.
#[test]
fn the_step_of_a_mark_counts_from_the_previous_mark_of_its_own_thread() {
    let marks = vec![
        Mark { name: "main_a", detail: String::new(), at_us: 1_000, thread: "main".into() },
        Mark { name: "worker_a", detail: String::new(), at_us: 4_000, thread: "worker".into() },
        Mark { name: "main_b", detail: String::new(), at_us: 6_000, thread: "main".into() },
        Mark { name: "worker_b", detail: String::new(), at_us: 9_000, thread: "worker".into() },
    ];
    let lines = render_table(&marks, None, 0);
    let row = |name: &str| lines.iter().find(|l| l.contains(name)).cloned().unwrap_or_default();
    assert!(row("main_b").contains("5.000"), "main_b is 5 ms after main_a, not 2 after worker_a: {}", row("main_b"));
    assert!(row("worker_b").contains("5.000"), "{}", row("worker_b"));
    assert!(row("worker_a").contains(" - "), "a thread's first mark has no step: {}", row("worker_a"));
}

/// With the process start known the times count from it, the library entry is the first row with
/// the step since the process start, and the origin line says where the number came from.
///
/// Mutation: the process age not added; the source missing; the first present's own line missing.
#[test]
fn times_count_from_the_process_start_when_the_os_said_it() {
    let age = ProcessAge { micros: 40_000, source: "sysctl kern.proc.pid", resolution_us: 1 };
    let marks = vec![
        Mark { name: "library_entered", detail: String::new(), at_us: 0, thread: "main".into() },
        Mark { name: "first_present", detail: String::new(), at_us: 100_000, thread: "main".into() },
    ];
    let lines = render_table(&marks, Some(age), 3);
    assert!(lines[0].contains("sysctl kern.proc.pid") && lines[0].contains("40.000"), "{}", lines[0]);
    let entered = lines.iter().find(|l| l.contains("library_entered")).expect("row");
    assert!(entered.contains("40.000"), "the library was entered 40 ms after the process start: {entered}");
    let present = lines.iter().find(|l| l.contains(" first_present")).expect("row");
    assert!(present.contains("140.000"), "{present}");
    assert!(lines.iter().any(|l| l.contains("first_present at 140.000 ms (100.000 ms after")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("layout passes begun: 3")));
    // Unknown process start: said, and the clock counts from the library entry.
    let unknown = render_table(&marks, None, 0);
    assert!(unknown[0].contains("unknown"), "{}", unknown[0]);
    assert!(unknown.iter().any(|l| l.contains("first_present at 100.000 ms")), "{unknown:?}");
}

/// The JSON is one object a script can parse: quotes and control characters in a detail are
/// escaped, the marks keep their order, the headline numbers are named.
///
/// Mutation: an unescaped quote; the headline missing; the times not from the process start.
#[test]
fn the_json_escapes_what_it_holds_and_names_the_headline_numbers() {
    assert_eq!(json_escape("a\"b\\c\nd\te\u{1}"), "a\\\"b\\\\c\\nd\\te\\u0001");
    let age = ProcessAge { micros: 10_000, source: "/proc/self/stat", resolution_us: 10_000 };
    let marks = vec![
        Mark { name: "library_entered", detail: String::new(), at_us: 0, thread: "main".into() },
        Mark { name: "first_present", detail: "say \"hi\"".into(), at_us: 90_000, thread: "main".into() },
    ];
    let json = render_json(true, &marks, Some(age), 1);
    assert!(json.starts_with("{\"enabled\":true,\"process_start_known\":true"), "{json}");
    assert!(json.contains("\"process_start_source\":\"/proc/self/stat\""), "{json}");
    assert!(json.contains("\"first_present_ms\":100.000"), "{json}");
    assert!(json.contains("\"detail\":\"say \\\"hi\\\"\""), "{json}");
    assert!(json.contains("\"layout_passes\":1"), "{json}");
    assert!(json.ends_with('}'));
    let off = StartupTimeline::disabled().to_json();
    assert!(off.starts_with("{\"enabled\":false"), "{off}");
    assert!(off.contains("\"marks\":[]"), "{off}");
}

/// A start that marks in a loop cannot grow the timeline without end.
///
/// Mutation: no bound.
#[test]
fn the_timeline_keeps_at_most_max_marks() {
    let t = StartupTimeline::new(true, Instant::now(), None);
    for _ in 0..MAX_MARKS + 50 {
        t.mark("loop");
    }
    assert_eq!(t.len(), MAX_MARKS);
}

/// A timeline made for the process starts with the library entry at 0 (a timeline that is off has
/// no marks); what the OS says of the process start is there where it says it.
///
/// Mutation: the entry mark missing; its time not 0; an off timeline that marks the entry.
#[test]
fn a_timeline_made_for_the_process_starts_with_the_library_entry_at_zero() {
    let on = StartupTimeline::for_process(true, Instant::now(), std::time::SystemTime::now());
    let marks = on.marks();
    assert_eq!(marks.len(), 1);
    assert_eq!((marks[0].name, marks[0].at_us), ("library_entered", 0));
    let known = cfg!(any(target_os = "macos", target_os = "ios", target_os = "linux", target_os = "android", target_os = "windows"));
    assert_eq!(on.before_origin().is_some(), known);
    let off = StartupTimeline::for_process(false, Instant::now(), std::time::SystemTime::now());
    assert!(off.is_empty() && !off.enabled());
}
