//! The events of a recording: the vocabulary, the caps, the lines. A mock clock and a `Vec` for a
//! file; no window, no azul, no disk.

use std::{collections::BTreeSet, sync::Arc};

use super::*;
use crate::recorder_clock::MockClock;

/// A sink, the log that writes into a `Vec`, and the clock that moves both.
fn rig(caps: Caps) -> (EventSink, EventLog<Vec<u8>>, MockClock) {
    let clock = MockClock::at(0);
    let (sink, rx) = event_channel(Arc::new(clock.clone()), caps);
    (sink, EventLog::new(Vec::new(), rx, 0), clock)
}

/// The lines the log has written, parsed.
fn lines(log: &EventLog<Vec<u8>>) -> Vec<serde_json::Value> {
    String::from_utf8(log.get_ref().clone())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
        .collect()
}

fn kinds_of(lines: &[serde_json::Value]) -> Vec<String> {
    lines.iter().map(|l| l["k"].as_str().unwrap().to_string()).collect()
}

// ---- the vocabulary ----

#[test]
fn every_kind_of_the_vocabulary_is_a_valid_unique_name_with_a_rate_and_fields() {
    let mut seen = BTreeSet::new();
    for k in &KINDS {
        assert!(is_kind_name(k.name), "{} is no kind name", k.name);
        assert!(seen.insert(k.name), "{} is listed twice", k.name);
        assert!(k.per_second >= 1 && k.burst >= 1, "{}: a kind that may send nothing", k.name);
        assert!(k.max_line >= 1_024, "{}: a line cap too small for any event", k.name);
        assert!(!k.fields.is_empty(), "{}: its fields are not described", k.name);
    }
    assert!(seen.contains("pen.stroke") && seen.contains("conv.word") && seen.contains("session.start"));
}

#[test]
fn the_recorder_doc_describes_every_kind_with_its_rate_and_nothing_else() {
    let doc = include_str!("../RECORDER.md");
    for k in &KINDS {
        let row = format!("| `{}` | {} |", k.name, k.per_second);
        assert!(doc.contains(&row), "RECORDER.md has no row {row:?}: the doc and the vocabulary differ");
    }
    // The rows of the vocabulary section name kinds the table has.
    let from = doc.find("### The vocabulary").expect("the doc has its vocabulary section");
    let to = doc[from..].find("## 5.").map_or(doc.len(), |i| from + i);
    let rows: Vec<&str> = doc[from..to].lines().filter(|l| l.starts_with("| `")).collect();
    assert_eq!(rows.len(), KINDS.len(), "one row for each kind, no more");
    for row in rows {
        let name = row.trim_start_matches("| `").split('`').next().unwrap();
        assert!(KINDS.iter().any(|k| k.name == name), "RECORDER.md describes {name}, which is no kind");
    }
}

#[test]
fn a_kind_is_looked_up_by_name_and_an_unknown_one_gets_the_default() {
    assert_eq!(spec_of("pointer.move").per_second, 20);
    assert_eq!(spec_of("pen.stroke").max_line, 128 * 1024);
    assert_eq!(spec_of("something.new"), &UNKNOWN_KIND);
}

#[test]
fn a_kind_name_is_lowercase_words_joined_by_dots() {
    for ok in ["marker", "pen.stroke", "pen.stroke.begin", "wb.cutout2", "a_b.c"] {
        assert!(is_kind_name(ok), "{ok}");
    }
    for bad in ["", ".a", "a.", "Pen.Stroke", "a b", "a\"b", "a/b", "ä", &"x".repeat(49)] {
        assert!(!is_kind_name(bad), "{bad:?}");
    }
}

// ---- lines ----

#[test]
fn an_event_becomes_one_json_line_with_the_time_the_kind_and_its_fields() {
    let (sink, mut log, clock) = rig(Caps::default());
    clock.advance_ms(1_500);
    sink.event("nav.file", r#"{"file":"src/lib.rs","lines":120}"#).unwrap();
    log.pump().unwrap();
    let text = String::from_utf8(log.get_ref().clone()).unwrap();
    assert_eq!(text, "{\"t\":1500000,\"k\":\"nav.file\",\"file\":\"src/lib.rs\",\"lines\":120}\n");
    assert_eq!(log.lines(), 1);
    assert_eq!(log.bytes(), text.len() as u64);
}

#[test]
fn an_event_without_fields_is_a_line_with_t_and_k() {
    let (sink, mut log, _) = rig(Caps::default());
    sink.event("marker", "{}").unwrap();
    log.pump().unwrap();
    assert_eq!(String::from_utf8(log.get_ref().clone()).unwrap(), "{\"t\":0,\"k\":\"marker\"}\n");
}

#[test]
fn an_event_is_stamped_where_it_is_made_not_where_it_is_written() {
    let (sink, mut log, clock) = rig(Caps::default());
    clock.set_us(1_000);
    sink.event("marker", "{}").unwrap();
    clock.set_us(9_000_000);
    sink.event_at(2_500, "marker", "{}").unwrap();
    log.pump().unwrap();
    let l = lines(&log);
    assert_eq!(l[0]["t"], 1_000);
    assert_eq!(l[1]["t"], 2_500, "a time the caller read earlier is kept");
}

#[test]
fn fields_named_t_or_k_cannot_overwrite_the_time_and_the_kind() {
    let (sink, mut log, clock) = rig(Caps::default());
    clock.set_us(7);
    sink.event("marker", r#"{"t":99,"k":"evil","label":"x"}"#).unwrap();
    log.pump().unwrap();
    let l = lines(&log);
    assert_eq!((l[0]["t"].as_u64(), l[0]["k"].as_str(), l[0]["label"].as_str()), (Some(7), Some("marker"), Some("x")));
}

#[test]
fn a_string_with_a_newline_in_it_stays_on_its_line() {
    let (sink, mut log, _) = rig(Caps::default());
    let fields = Fields::new().string("label", "line one\nline \"two\"\ttab").build();
    sink.event("marker", &fields).unwrap();
    log.pump().unwrap();
    let text = String::from_utf8(log.get_ref().clone()).unwrap();
    assert_eq!(text.matches('\n').count(), 1, "one line: {text:?}");
    assert_eq!(lines(&log)[0]["label"], "line one\nline \"two\"\ttab");
}

#[test]
fn a_line_of_every_kind_of_the_vocabulary_parses() {
    let (sink, mut log, clock) = rig(Caps::default());
    for k in &KINDS {
        clock.advance_ms(100);
        sink.event(k.name, "{}").unwrap();
    }
    log.pump().unwrap();
    let l = lines(&log);
    assert_eq!(l.len(), KINDS.len());
    assert_eq!(kinds_of(&l), KINDS.iter().map(|k| k.name.to_string()).collect::<Vec<_>>());
}

// ---- invalid input ----

#[test]
fn fields_that_are_not_one_json_object_are_dropped_and_said_so() {
    let (sink, mut log, clock) = rig(Caps::default());
    for bad in ["", "{", "[1,2]", "5", "\"x\"", "{\"a\":}", "null"] {
        sink.event("marker", bad).unwrap(); // accepted at the door: the writer thread checks
    }
    sink.event("marker", r#"{"ok":1}"#).unwrap();
    clock.advance_ms(2_000);
    log.pump().unwrap();
    let l = lines(&log);
    assert_eq!(kinds_of(&l), vec!["marker", "recorder.drop"]);
    assert_eq!(l[1]["kind"], "marker");
    assert_eq!(l[1]["why"], "invalid");
    assert_eq!(l[1]["n"], 7);
}

#[test]
fn a_name_that_is_no_kind_is_refused_at_once() {
    let (sink, _log, _) = rig(Caps::default());
    assert_eq!(sink.event("Bad Name", "{}"), Err(Dropped::Invalid));
    assert_eq!(sink.event("", "{}"), Err(Dropped::Invalid));
    assert_eq!(sink.dropped_total(), 2);
    assert_eq!(sink.accepted(), 0);
}

// ---- the caps ----

#[test]
fn a_kind_may_send_its_burst_at_once_and_then_its_rate() {
    let (sink, _log, clock) = rig(Caps::default());
    // pointer.move: 20 a second, a burst of 20.
    let sent = |sink: &EventSink| (0..100).filter(|_| sink.event("pointer.move", "{}").is_ok()).count();
    assert_eq!(sent(&sink), 20, "the burst");
    clock.advance_ms(500);
    assert_eq!(sent(&sink), 10, "half a second brings ten tokens back");
    clock.advance_ms(10_000);
    assert_eq!(sent(&sink), 20, "never more than the burst, however long the wait");
    assert_eq!(sink.dropped_total(), 80 + 90 + 80);
}

#[test]
fn one_kind_over_its_rate_does_not_take_the_others_share() {
    let (sink, _log, _) = rig(Caps::default());
    for _ in 0..1_000 {
        let _ = sink.event("pointer.move", "{}");
    }
    assert!(sink.event("nav.file", "{}").is_ok());
    assert!(sink.event("marker", "{}").is_ok());
}

#[test]
fn all_kinds_together_may_not_send_more_than_the_total() {
    let caps = Caps { total_per_second: 10, total_burst: 10, ..Caps::default() };
    let (sink, mut log, clock) = rig(caps);
    let names = ["nav.file", "nav.page", "pen.select", "marker", "wb.open", "conv.done", "layout.toggle", "dictation.end", "wb.close", "conv.reject", "wb.jump", "diff.accept"];
    let results: Vec<_> = names.iter().map(|n| sink.event(n, "{}")).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 10);
    assert_eq!(results[10], Err(Dropped::Total));
    clock.advance_ms(1_000);
    log.pump().unwrap();
    let l = lines(&log);
    let drop = l.iter().find(|x| x["k"] == "recorder.drop").expect("the drops are said");
    assert_eq!(drop["why"], "rate");
}

#[test]
fn the_queue_is_bounded_and_drains_when_the_writer_runs() {
    let caps = Caps { queue: 5, ..Caps::default() };
    let (sink, mut log, clock) = rig(caps);
    let mut accepted = 0;
    let mut refused = 0;
    for i in 0..20 {
        clock.advance_ms(100 * i);
        match sink.event("marker", "{}") {
            Ok(()) => accepted += 1,
            Err(Dropped::Queue) => refused += 1,
            Err(other) => panic!("{other:?}"),
        }
    }
    assert_eq!((accepted, refused), (5, 15), "the writer did not run, so only five wait");
    log.pump().unwrap();
    assert!(sink.event("marker", "{}").is_ok(), "the queue drained");
}

#[test]
fn a_line_over_its_kinds_size_is_dropped_and_a_stroke_may_be_long() {
    let (sink, mut log, clock) = rig(Caps::default());
    let long = Fields::new().string("label", &"x".repeat(9_000)).build();
    assert_eq!(sink.event("marker", &long), Err(Dropped::Size));
    let samples = format!("[{}]", (0..2_000).map(|i| format!("[{i},12.5,34.5,0.5]")).collect::<Vec<_>>().join(","));
    let stroke = Fields::new().int("id", 1).raw("s", &samples).build();
    assert!(stroke.len() > 30_000);
    assert_eq!(sink.event("pen.stroke", &stroke), Ok(()), "2000 samples fit the stroke's cap");
    clock.advance_ms(1_000);
    log.pump().unwrap();
    let l = lines(&log);
    assert_eq!(l[0]["k"], "pen.stroke");
    assert_eq!(l[0]["s"].as_array().unwrap().len(), 2_000);
}

#[test]
fn a_kind_the_vocabulary_does_not_know_is_recorded_at_the_default_rate() {
    let (sink, mut log, _) = rig(Caps::default());
    let sent = (0..50).filter(|_| sink.event("future.thing", r#"{"a":1}"#).is_ok()).count();
    assert_eq!(sent, 20, "the default burst");
    log.pump().unwrap();
    assert_eq!(lines(&log).len(), 20);
}

#[test]
fn the_file_stops_growing_at_its_cap_but_the_recorders_own_lines_still_land() {
    let caps = Caps { file_bytes: 400, ..Caps::default() };
    let (sink, mut log, clock) = rig(caps);
    for i in 0..30 {
        clock.advance_ms(60);
        sink.event("nav.page", &Fields::new().int("page", i).string("file", "src/some/file.rs").build()).unwrap();
    }
    sink.event("session.stop", r#"{"reason":"user"}"#).unwrap();
    clock.advance_ms(2_000);
    log.pump().unwrap();
    let l = lines(&log);
    assert!(log.bytes() <= 400 + 400, "the file stayed near its cap: {}", log.bytes());
    let ks = kinds_of(&l);
    assert!(ks.iter().filter(|k| *k == "nav.page").count() < 30);
    assert_eq!(ks.iter().filter(|k| *k == "recorder.events_capped").count(), 1, "said once");
    assert_eq!(ks.last().map(String::as_str), Some("recorder.drop"));
    assert!(ks.contains(&"session.stop".to_string()), "the recorder's own lines still land");
    let capped = l.iter().find(|x| x["k"] == "recorder.drop").unwrap();
    assert_eq!(capped["why"], "capped");
}

// ---- what was dropped is said once, as a delta ----

#[test]
fn the_log_reports_what_was_dropped_once_a_second_as_the_difference() {
    let (sink, mut log, clock) = rig(Caps::default());
    for _ in 0..30 {
        let _ = sink.event("pointer.move", "{}"); // 20 pass, 10 are dropped
    }
    clock.advance_ms(1_100);
    log.pump().unwrap();
    let first = lines(&log).into_iter().filter(|l| l["k"] == "recorder.drop").collect::<Vec<_>>();
    assert_eq!(first.len(), 1);
    assert_eq!((first[0]["kind"].as_str(), first[0]["n"].as_u64()), (Some("pointer.move"), Some(10)));

    clock.advance_ms(1_100);
    log.pump().unwrap();
    let again = lines(&log).into_iter().filter(|l| l["k"] == "recorder.drop").count();
    assert_eq!(again, 1, "nothing new was dropped, so nothing new is said");

    for _ in 0..40 {
        let _ = sink.event("pointer.move", "{}"); // the bucket refilled to 20: 20 pass, 20 dropped
    }
    clock.advance_ms(1_100);
    log.pump().unwrap();
    let all: Vec<_> = lines(&log).into_iter().filter(|l| l["k"] == "recorder.drop").collect();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1]["n"], 20, "the delta, not the running total");
}

// ---- the writer's end ----

#[test]
fn a_flush_message_flushes_the_file() {
    struct Counting(Vec<u8>, usize);
    impl Write for Counting {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.1 += 1;
            Ok(())
        }
    }
    let clock = MockClock::at(0);
    let (sink, rx) = event_channel(Arc::new(clock), Caps::default());
    let mut log = EventLog::new(Counting(Vec::new(), 0), rx, 0);
    sink.event("marker", "{}").unwrap();
    log.pump().unwrap();
    assert_eq!(log.get_ref().1, 0, "a few lines do not flush by themselves");
    sink.flush();
    log.pump().unwrap();
    assert_eq!(log.get_ref().1, 1);
}

#[test]
fn the_log_says_when_every_sink_is_gone_and_flushes_then() {
    let (sink, mut log, _) = rig(Caps::default());
    sink.event("marker", "{}").unwrap();
    let pumped = log.pump().unwrap();
    assert_eq!(pumped, Pumped { written: 1, closed: false });
    drop(sink);
    let done = log.pump().unwrap();
    assert!(done.closed);
}

#[test]
fn a_file_that_already_has_lines_counts_them_in_its_size() {
    let clock = MockClock::at(0);
    let (sink, rx) = event_channel(Arc::new(clock), Caps { file_bytes: 1_000, ..Caps::default() });
    let mut log = EventLog::new(Vec::new(), rx, 990);
    sink.event("marker", r#"{"label":"this line does not fit"}"#).unwrap();
    log.pump().unwrap();
    let l = lines(&log);
    assert_eq!(kinds_of(&l), vec!["recorder.events_capped"]);
}

#[test]
fn several_threads_may_send_at_once() {
    let (sink, mut log, _) = rig(Caps::default());
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let sink = sink.clone();
            std::thread::spawn(move || {
                let kind = format!("thread.t{n}");
                (0..20).filter(|_| sink.event(&kind, "{}").is_ok()).count()
            })
        })
        .collect();
    let accepted: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(accepted, 160, "each thread's kind has a burst of 20");
    log.pump().unwrap();
    assert_eq!(lines(&log).len(), 160);
}

// ---- building fields ----

#[test]
fn the_builder_makes_one_valid_object() {
    let fields = Fields::new()
        .int("id", 5)
        .string("file", "a\"b.rs")
        .flag("ok", true)
        .num("x", 1.5)
        .num("whole", 3.0)
        .raw("s", "[1,2]")
        .build();
    assert_eq!(fields, r#"{"id":5,"file":"a\"b.rs","ok":true,"x":1.5,"whole":3,"s":[1,2]}"#);
    assert_eq!(Fields::new().build(), "{}");
    let v: serde_json::Value = serde_json::from_str(&fields).unwrap();
    assert_eq!(v["file"], "a\"b.rs");
}

#[test]
fn numbers_are_short_finite_and_never_in_exponent_form() {
    assert_eq!(json_number(5.0), "5");
    assert_eq!(json_number(1.5), "1.5");
    assert_eq!(json_number(0.12345), "0.123");
    assert_eq!(json_number(-0.0001), "0");
    assert_eq!(json_number(-2.25), "-2.25");
    assert_eq!(json_number(1e9), "1000000000");
    assert_eq!(json_number(f64::NAN), "0");
    assert_eq!(json_number(f64::INFINITY), "0");
}

#[test]
fn strings_are_escaped_for_json() {
    assert_eq!(json_string("a\"b\\c\nd\te\u{1}f"), "\"a\\\"b\\\\c\\nd\\te\\u0001f\"");
    assert_eq!(json_string("übung ✓"), "\"übung ✓\"");
}

#[test]
fn the_members_of_an_object_come_out_without_braces_and_without_t_and_k() {
    assert_eq!(object_members(r#"{"a":1,"t":2,"b":"x"}"#).as_deref(), Some(r#""a":1,"b":"x""#));
    assert_eq!(object_members("{}").as_deref(), Some(""));
    assert_eq!(object_members("[]"), None);
}
