//! Undo / redo of whole-state snapshots, for any app state `T` (DEDUP_OFFICE
//! D10: AzPhoto, AzVideoCut and AzShow each had their own diverged copy).
//!
//! The app owns the CURRENT state; the history holds the states BEFORE each
//! edit (the undo steps) and the states an undo stepped back from (the redo
//! steps), each named by its edit ("Move", "Brush") for the Edit menu ("Undo
//! Move") and a History panel.
//!
//! - [`UndoHistory::checkpoint`] before an edit records the state it starts
//!   from and drops the redo branch;
//! - [`UndoHistory::checkpoint_with`] does the same lazily, and with a
//!   COALESCING key a run of edits with that key (a dragged slider, the steps
//!   of one drag) is ONE step until [`UndoHistory::seal`], an undo or an edit
//!   without that key ends the run - the snapshot is not even built for the
//!   merged edits;
//! - [`UndoHistory::undo`] / [`UndoHistory::redo`] swap the app's state with
//!   the step's; [`UndoHistory::jump`] goes to any state of the panel;
//! - past [`UndoHistory::limit`] steps the oldest goes (a `VecDeque`: no
//!   memmove of the whole history per edit).
//!
//! The panel lists [`UndoHistory::labels`]: the oldest state kept (named
//! `base_label` - "Open", "New" - until it is dropped, then by the edit that
//! made the oldest state kept), then one entry per step; the current state is
//! [`UndoHistory::current_index`].
//!
//! Plain Rust (a snapshot of an app type cannot cross azul's C API); tested
//! without a window.

use std::collections::VecDeque;

/// A state and the edit that leaves it (undo) or led to it (redo).
#[derive(Debug, Clone)]
struct Step<T> {
    label: String,
    state: T,
    /// The coalescing key of the run this undo step opened.
    coalesce: Option<String>,
}

/// The undo and redo steps of one document.
#[derive(Debug, Clone)]
pub struct UndoHistory<T> {
    /// The states before each edit, oldest first.
    undo: VecDeque<Step<T>>,
    /// The states undone, the next redo LAST.
    redo: Vec<Step<T>>,
    /// The most undo steps kept.
    limit: usize,
    /// The name of the oldest state kept.
    base_label: String,
    /// A coalescing run is open on the top undo step.
    open: bool,
}

impl<T> UndoHistory<T> {
    /// The undo steps kept unless [`Self::with_limit`] says otherwise.
    pub const DEFAULT_LIMIT: usize = 100;

    /// An empty history whose starting state is named `base_label` ("Open").
    #[must_use]
    pub fn new(base_label: &str) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: Self::DEFAULT_LIMIT,
            base_label: base_label.to_string(),
            open: false,
        }
    }

    /// Keep at most `limit` undo steps (at least one).
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// The most undo steps kept.
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    /// Records `before`, the state an edit named `label` starts from; the
    /// redo branch goes. Ends a coalescing run.
    pub fn checkpoint(&mut self, label: &str, before: T) {
        self.checkpoint_with(label, None, || before);
    }

    /// [`Self::checkpoint`] with the snapshot built only when it is
    /// recorded: with `coalesce`, an edit carrying the same key as the run
    /// still open on the top step is part of that step (`before` is not
    /// called). Returns whether a step was recorded.
    pub fn checkpoint_with(
        &mut self,
        label: &str,
        coalesce: Option<&str>,
        before: impl FnOnce() -> T,
    ) -> bool {
        if let Some(key) = coalesce {
            let merges = self.open
                && self
                    .undo
                    .back()
                    .is_some_and(|top| top.coalesce.as_deref() == Some(key));
            if merges {
                return false;
            }
        }
        self.redo.clear();
        self.undo.push_back(Step {
            label: label.to_string(),
            state: before(),
            coalesce: coalesce.map(str::to_string),
        });
        self.open = coalesce.is_some();
        while self.undo.len() > self.limit {
            if let Some(oldest) = self.undo.pop_front() {
                // The oldest state kept is the one that edit made.
                self.base_label = oldest.label;
            }
        }
        true
    }

    /// Ends the coalescing run: the next edit is a step of its own.
    pub fn seal(&mut self) {
        self.open = false;
    }

    /// Whether there is an edit to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is an undone edit to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The name of the edit an undo takes back ("Undo Move").
    #[must_use]
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|s| s.label.as_str())
    }

    /// The name of the edit a redo makes again.
    #[must_use]
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    /// One edit back: `current` becomes the state before it (the state left
    /// is kept for the redo). `false` when there is nothing to undo.
    pub fn undo(&mut self, current: &mut T) -> bool {
        let Some(step) = self.undo.pop_back() else {
            return false;
        };
        let after = core::mem::replace(current, step.state);
        self.redo.push(Step {
            label: step.label,
            state: after,
            coalesce: None,
        });
        self.open = false;
        true
    }

    /// One undone edit forward again. `false` when there is nothing to redo.
    pub fn redo(&mut self, current: &mut T) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        let before = core::mem::replace(current, step.state);
        self.undo.push_back(Step {
            label: step.label,
            state: before,
            coalesce: None,
        });
        self.open = false;
        true
    }

    /// Every state's name, oldest first: the History panel's rows.
    #[must_use]
    pub fn labels(&self) -> Vec<String> {
        let mut labels = Vec::with_capacity(1 + self.undo.len() + self.redo.len());
        labels.push(self.base_label.clone());
        labels.extend(self.undo.iter().map(|s| s.label.clone()));
        labels.extend(self.redo.iter().rev().map(|s| s.label.clone()));
        labels
    }

    /// The row of [`Self::labels`] the current state is.
    #[must_use]
    pub fn current_index(&self) -> usize {
        self.undo.len()
    }

    /// Goes to row `index` of [`Self::labels`] (a click in the History
    /// panel), undoing or redoing the edits between. `false` when there is
    /// no such row.
    pub fn jump(&mut self, index: usize, current: &mut T) -> bool {
        if index > self.undo.len() + self.redo.len() {
            return false;
        }
        while self.undo.len() > index {
            self.undo(current);
        }
        while self.undo.len() < index {
            self.redo(current);
        }
        true
    }

    /// Forgets every step (a document opened anew); the starting state is
    /// named `base_label`.
    pub fn clear(&mut self, base_label: &str) {
        self.undo.clear();
        self.redo.clear();
        self.base_label = base_label.to_string();
        self.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undo_restores_the_state_before_the_edit_and_a_redo_the_one_after() {
        let mut doc = 1;
        let mut h = UndoHistory::new("Open");
        assert!(!h.can_undo() && !h.can_redo());
        h.checkpoint("Add", doc);
        doc = 2;
        h.checkpoint("Double", doc);
        doc = 4;
        assert!(h.undo(&mut doc));
        assert_eq!(doc, 2);
        assert!(h.undo(&mut doc));
        assert_eq!(doc, 1);
        assert!(!h.undo(&mut doc), "nothing left to undo");
        assert_eq!(doc, 1, "and the state stays");
        assert!(h.redo(&mut doc));
        assert_eq!(doc, 2);
        assert!(h.redo(&mut doc));
        assert_eq!(doc, 4);
        assert!(!h.redo(&mut doc));
        assert_eq!(doc, 4);
    }

    #[test]
    fn a_new_edit_after_an_undo_drops_the_redo_branch() {
        let mut doc = String::from("a");
        let mut h = UndoHistory::new("Open");
        h.checkpoint("Type b", doc.clone());
        doc.push('b');
        assert!(h.undo(&mut doc));
        assert!(h.can_redo());
        h.checkpoint("Type c", doc.clone());
        doc.push('c');
        assert!(!h.can_redo(), "a new edit forgets the undone one");
        assert!(h.undo(&mut doc));
        assert_eq!(doc, "a");
    }

    #[test]
    fn undo_and_redo_labels_name_the_next_step() {
        let mut doc = 0;
        let mut h = UndoHistory::new("Open");
        assert_eq!(h.undo_label(), None);
        h.checkpoint("Move", doc);
        doc = 1;
        h.checkpoint("Resize", doc);
        doc = 2;
        assert_eq!(h.undo_label(), Some("Resize"));
        assert!(h.undo(&mut doc));
        assert_eq!(h.undo_label(), Some("Move"));
        assert_eq!(h.redo_label(), Some("Resize"));
    }

    #[test]
    fn edits_with_the_same_coalescing_key_are_one_step_until_sealed_or_undone() {
        let mut doc = 10;
        let mut h = UndoHistory::new("Open");
        for v in [11, 12, 13] {
            h.checkpoint_with("Opacity", Some("opacity"), || doc);
            doc = v;
        }
        assert_eq!(h.labels(), vec!["Open", "Opacity"], "the drag is one step");
        assert!(h.undo(&mut doc));
        assert_eq!(doc, 10, "the step goes back to before the whole run");
        assert!(h.redo(&mut doc));
        // The run is over after an undo / redo: the same key starts a new step.
        h.checkpoint_with("Opacity", Some("opacity"), || doc);
        doc = 20;
        h.seal();
        h.checkpoint_with("Opacity", Some("opacity"), || doc);
        doc = 30;
        assert_eq!(h.labels(), vec!["Open", "Opacity", "Opacity", "Opacity"]);
        // Another key, or none, is a step of its own.
        h.checkpoint_with("Blur", Some("blur"), || doc);
        h.checkpoint_with("Blur", None, || doc);
        assert_eq!(h.labels().len(), 6);
    }

    #[test]
    fn checkpoint_with_does_not_build_the_snapshot_when_it_merges() {
        let mut built = 0;
        let mut h: UndoHistory<u32> = UndoHistory::new("Open");
        assert!(h.checkpoint_with("Drag", Some("drag"), || {
            built += 1;
            0
        }));
        assert!(!h.checkpoint_with("Drag", Some("drag"), || {
            built += 1;
            1
        }));
        assert_eq!(built, 1);
    }

    #[test]
    fn the_oldest_steps_go_past_the_limit_and_the_panel_starts_at_the_oldest_kept_state() {
        let mut doc = 0;
        let mut h = UndoHistory::new("Open").with_limit(2);
        for (i, label) in ["One", "Two", "Three"].iter().enumerate() {
            h.checkpoint(label, doc);
            doc = i + 1;
        }
        assert_eq!(
            h.labels(),
            vec!["One", "Two", "Three"],
            "the state before One is gone"
        );
        assert!(h.undo(&mut doc) && h.undo(&mut doc));
        assert!(!h.undo(&mut doc));
        assert_eq!(doc, 1, "the oldest state kept is the one One made");
        assert_eq!(UndoHistory::<u8>::new("x").with_limit(0).limit(), 1);
    }

    #[test]
    fn the_labels_name_every_state_and_jump_moves_to_any_of_them() {
        let mut doc = 0;
        let mut h = UndoHistory::new("Open");
        for (i, label) in ["A", "B", "C"].iter().enumerate() {
            h.checkpoint(label, doc);
            doc = i + 1;
        }
        assert_eq!(h.labels(), vec!["Open", "A", "B", "C"]);
        assert_eq!(h.current_index(), 3);
        assert!(h.jump(1, &mut doc));
        assert_eq!((doc, h.current_index()), (1, 1));
        assert_eq!(
            h.labels(),
            vec!["Open", "A", "B", "C"],
            "the rows stay while undone"
        );
        assert!(h.jump(3, &mut doc));
        assert_eq!((doc, h.current_index()), (3, 3));
        assert!(h.jump(0, &mut doc));
        assert_eq!(doc, 0);
        assert!(!h.jump(9, &mut doc));
        h.clear("New");
        assert_eq!(h.labels(), vec!["New"]);
        assert!(!h.can_undo() && !h.can_redo());
    }
}
