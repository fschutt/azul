//! The whiteboard's changes as events of the recording.
//!
//! The board (`whiteboard.rs`) tells whoever is listening what happens on it ([`BoardObserver`]); this
//! observer turns each change into one event of the recording's vocabulary (`wb.cutout`, `wb.arrow`,
//! `wb.edit`, `wb.jump`) and sends it to the session's event sink. With no session going the sink is
//! `None` and the observer hears and says nothing. Plain data in, a line of JSON out; the mapping is
//! one pure function.

use crate::{
    recorder_events::{EventSink, Fields},
    whiteboard::{BoardEvent, BoardObserver},
};

/// The kind and the fields (a JSON object as text) of the event a board change is.
#[must_use]
pub fn wb_event(event: &BoardEvent) -> (&'static str, String) {
    let id = |f: Fields, id: u64| f.int("id", i64::try_from(id).unwrap_or(i64::MAX));
    let edit = |op: &str, item: Option<u64>| {
        let f = Fields::new().string("op", op);
        ("wb.edit", item.map_or(f.clone(), |i| id(f, i)).build())
    };
    match event {
        BoardEvent::CutoutCreated { id: i, file, first_line, last_line } => (
            "wb.cutout",
            id(Fields::new(), *i).string("file", file).int("first", i64::from(*first_line)).int("last", i64::from(*last_line)).build(),
        ),
        BoardEvent::ArrowDrawn { id: i, from, to } => {
            let mut f = id(Fields::new(), *i);
            if let Some(from) = from {
                f = id_named(f, "from", *from);
            }
            if let Some(to) = to {
                f = id_named(f, "to", *to);
            }
            ("wb.arrow", f.build())
        }
        BoardEvent::ArrowChanged { id: i } => edit("arrow", Some(*i)),
        BoardEvent::CutoutMoved { id: i } => edit("moved", Some(*i)),
        BoardEvent::CutoutResized { id: i } => edit("resized", Some(*i)),
        BoardEvent::Labelled { id: i } => edit("labelled", Some(*i)),
        BoardEvent::Removed { ids } => {
            let list = format!("[{}]", ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(","));
            ("wb.edit", Fields::new().string("op", "removed").raw("ids", &list).build())
        }
        BoardEvent::Undone => edit("undone", None),
        BoardEvent::Redone => edit("redone", None),
        BoardEvent::Jumped { id: i, file, first_line, last_line } => (
            "wb.jump",
            id(Fields::new(), *i).string("file", file).int("first", i64::from(*first_line)).int("last", i64::from(*last_line)).build(),
        ),
    }
}

fn id_named(f: Fields, key: &str, value: u64) -> Fields {
    f.int(key, i64::try_from(value).unwrap_or(i64::MAX))
}

/// Records what happens on a whiteboard.
#[derive(Clone, Default)]
pub struct WbRecorder {
    sink: Option<EventSink>,
}

impl WbRecorder {
    /// An observer that sends to `sink` (`None`: nothing is recorded).
    #[must_use]
    pub const fn new(sink: Option<EventSink>) -> Self {
        Self { sink }
    }

    /// A session began or ended.
    pub fn set_sink(&mut self, sink: Option<EventSink>) {
        self.sink = sink;
    }
}

impl BoardObserver for WbRecorder {
    fn on_event(&mut self, event: &BoardEvent) {
        if let Some(sink) = &self.sink {
            let (kind, fields) = wb_event(event);
            let _ = sink.event(kind, &fields);
        }
    }
}

#[cfg(test)]
#[path = "recorder_wb_test.rs"]
mod recorder_wb_test;
