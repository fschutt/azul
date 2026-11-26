//! What the window tells a recorder about what happens in it: ONE call, `event(kind, fields)`, in the recorder's own
//! vocabulary (`recorder_events::KINDS`: `layout.toggle`, `nav.file`, `pen.select`, ...). The default does nothing, so
//! a window with no recorder pays nothing; the recorder's `EventSink` is a hook (its call stamps the event with the
//! session clock and never blocks).
//!
//! The calls are made where the person acts (a pen taken, a file opened, a panel opened), never in the path that draws
//! a frame.

use std::sync::Arc;

use crate::recorder_events::EventSink;

/// Where the window's events go.
pub trait RecorderHook: Send + Sync {
    /// Something happened: its `kind` and its `fields` (one JSON object).
    fn event(&self, kind: &str, fields: &str);
}

/// The hook that is nobody: no recorder.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoRecorder;

impl RecorderHook for NoRecorder {
    fn event(&self, _kind: &str, _fields: &str) {}
}

impl RecorderHook for EventSink {
    fn event(&self, kind: &str, fields: &str) {
        // An event over a cap is dropped and counted by the sink itself (`recorder.drop`).
        let _ = EventSink::event(self, kind, fields);
    }
}

/// The hook a window starts with.
#[must_use]
pub fn none() -> Arc<dyn RecorderHook> {
    Arc::new(NoRecorder)
}

#[cfg(test)]
#[path = "recorder_hook_test.rs"]
mod recorder_hook_test;
