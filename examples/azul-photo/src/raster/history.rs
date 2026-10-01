//! Undo history: whole document states, cheap because tiles are shared.
//!
//! Each state is a [`Document`] clone: its layers' tiles are `Arc`s, so a
//! state costs only the tiles its edit copied (a brush dab: one tile). A run
//! of edits with the same coalescing key (a dragged opacity slider) replaces
//! its own top state instead of piling up one per value.

use super::document::Document;

#[derive(Clone)]
struct State {
    label: String,
    doc: Document,
    coalesce: Option<String>,
}

/// The History panel's list.
pub struct History {
    states: Vec<State>,
    current: usize,
    limit: usize,
}

impl History {
    /// States kept before the oldest are dropped.
    pub const DEFAULT_LIMIT: usize = 60;

    /// A history whose first state is `doc` ("Open", "New").
    #[must_use]
    pub fn new(label: &str, doc: Document) -> Self {
        Self {
            states: vec![State {
                label: label.to_string(),
                doc,
                coalesce: None,
            }],
            current: 0,
            limit: Self::DEFAULT_LIMIT,
        }
    }

    /// The state shown now.
    #[must_use]
    pub fn current(&self) -> &Document {
        &self.states[self.current].doc
    }

    #[must_use]
    pub const fn current_index(&self) -> usize {
        self.current
    }

    /// Every state's label, oldest first.
    #[must_use]
    pub fn labels(&self) -> Vec<String> {
        self.states.iter().map(|s| s.label.clone()).collect()
    }

    /// Record the document after an edit. A state after an undo drops the
    /// redo branch. With `coalesce`, an edit carrying the same key as the
    /// state on top (made just before, nothing undone since) replaces it.
    pub fn push(&mut self, label: &str, doc: Document, coalesce: Option<String>) {
        if let Some(key) = coalesce.as_deref() {
            let top = self.current + 1 == self.states.len();
            if top && self.current > 0 && self.states[self.current].coalesce.as_deref() == Some(key) {
                self.states[self.current].doc = doc;
                return;
            }
        }
        self.states.truncate(self.current + 1);
        self.states.push(State {
            label: label.to_string(),
            doc,
            coalesce,
        });
        if self.states.len() > self.limit {
            let excess = self.states.len() - self.limit;
            self.states.drain(0..excess);
        }
        self.current = self.states.len() - 1;
    }

    #[must_use]
    pub const fn can_undo(&self) -> bool {
        self.current > 0
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.current + 1 < self.states.len()
    }

    /// One state back.
    pub fn undo(&mut self) -> Option<&Document> {
        if !self.can_undo() {
            return None;
        }
        self.current -= 1;
        Some(&self.states[self.current].doc)
    }

    /// One state forward.
    pub fn redo(&mut self) -> Option<&Document> {
        if !self.can_redo() {
            return None;
        }
        self.current += 1;
        Some(&self.states[self.current].doc)
    }

    /// Show state `index` (a click in the History panel).
    pub fn jump(&mut self, index: usize) -> Option<&Document> {
        if index >= self.states.len() {
            return None;
        }
        self.current = index;
        Some(&self.states[index].doc)
    }
}
