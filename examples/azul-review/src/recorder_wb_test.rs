//! The whiteboard's changes as the lines of a recording. No window, no azul.

use std::sync::Arc;

use super::*;
use crate::{
    recorder_clock::MockClock,
    recorder_events::{event_channel, spec_of, Caps, EventLog, UNKNOWN_KIND},
};

fn all_events() -> Vec<BoardEvent> {
    vec![
        BoardEvent::CutoutCreated { id: 1, file: String::from("src/lib.rs"), first_line: 10, last_line: 40 },
        BoardEvent::ArrowDrawn { id: 2, from: Some(1), to: Some(3) },
        BoardEvent::ArrowDrawn { id: 4, from: None, to: None },
        BoardEvent::ArrowChanged { id: 2 },
        BoardEvent::CutoutMoved { id: 1 },
        BoardEvent::CutoutResized { id: 1 },
        BoardEvent::Labelled { id: 1 },
        BoardEvent::Removed { ids: vec![1, 2] },
        BoardEvent::Undone,
        BoardEvent::Redone,
        BoardEvent::Jumped { id: 1, file: String::from("a \"quoted\".rs"), first_line: 1, last_line: 2 },
    ]
}

#[test]
fn every_change_of_the_board_is_a_kind_of_the_vocabulary_with_fields_that_parse() {
    for e in all_events() {
        let (kind, fields) = wb_event(&e);
        assert_ne!(spec_of(kind), &UNKNOWN_KIND, "{kind} is in the vocabulary ({e:?})");
        let v: serde_json::Value = serde_json::from_str(&fields).unwrap_or_else(|err| panic!("{fields}: {err}"));
        assert!(v.is_object(), "{fields}");
    }
}

#[test]
fn the_fields_say_what_the_vocabulary_says() {
    let (kind, f) = wb_event(&BoardEvent::CutoutCreated { id: 7, file: String::from("src/lib.rs"), first_line: 10, last_line: 40 });
    assert_eq!((kind, f.as_str()), ("wb.cutout", r#"{"id":7,"file":"src/lib.rs","first":10,"last":40}"#));
    let (kind, f) = wb_event(&BoardEvent::ArrowDrawn { id: 2, from: Some(1), to: None });
    assert_eq!((kind, f.as_str()), ("wb.arrow", r#"{"id":2,"from":1}"#), "an end on nothing is left out");
    let (kind, f) = wb_event(&BoardEvent::Removed { ids: vec![3, 4] });
    assert_eq!((kind, f.as_str()), ("wb.edit", r#"{"op":"removed","ids":[3,4]}"#));
    let (_, f) = wb_event(&BoardEvent::Undone);
    assert_eq!(f, r#"{"op":"undone"}"#);
    let (kind, f) = wb_event(&BoardEvent::Jumped { id: 1, file: String::from("a.rs"), first_line: 5, last_line: 9 });
    assert_eq!((kind, f.as_str()), ("wb.jump", r#"{"id":1,"file":"a.rs","first":5,"last":9}"#));
}

#[test]
fn an_observer_with_a_sink_records_and_one_without_hears_nothing() {
    let clock = MockClock::at(0);
    let (sink, rx) = event_channel(Arc::new(clock.clone()), Caps::default());
    let mut log = EventLog::new(Vec::<u8>::new(), rx, 0);
    let mut observer = WbRecorder::new(Some(sink));
    clock.advance_ms(2_500);
    observer.on_event(&BoardEvent::CutoutCreated { id: 1, file: String::from("src/lib.rs"), first_line: 1, last_line: 9 });
    observer.on_event(&BoardEvent::Undone);
    log.pump().unwrap();
    let text = String::from_utf8(log.get_ref().clone()).unwrap();
    let kinds: Vec<&str> = text.lines().map(|l| l.split("\"k\":\"").nth(1).unwrap().split('"').next().unwrap()).collect();
    assert_eq!(kinds, vec!["wb.cutout", "wb.edit"]);
    assert!(text.starts_with("{\"t\":2500000,\"k\":\"wb.cutout\""), "{text}");

    observer.set_sink(None);
    observer.on_event(&BoardEvent::Undone);
    log.pump().unwrap();
    assert_eq!(String::from_utf8(log.get_ref().clone()).unwrap().lines().count(), 2, "no session: nothing");
    WbRecorder::default().on_event(&BoardEvent::Redone);
}
