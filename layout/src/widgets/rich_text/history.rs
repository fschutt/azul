//! The rich-text editor's ONE undo / redo history.
//!
//! `AzWriter` kept two (its own stack of inverse edits AND the engine's,
//! handed the same inverse; Ctrl+Z and the Undo button undid different
//! histories - `scripts/DEDUP_EDITORS` A3.6). The shared editor keeps one:
//! a snapshot of the document before every step - typing (consecutive
//! typing in one block is one step), Enter, Backspace across blocks, a
//! paste, a format, a block kind, an indent, a link, a tick. Undo puts the
//! snapshot back and the editor resets the engine's editing state for the
//! host (`CallbackInfo::reset_editor_content`), which drops the engine's
//! own per-host histories, so nothing else is left to undo.

use super::doc::{RichTextDoc, RichTextDocVec};

/// Which kind of step the last recorded one was: consecutive steps of the
/// same group are one undo step.
#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichEditGroup {
    /// A step of its own (never merged with the next).
    #[default]
    None,
    /// Typing into block `index`.
    Typing(usize),
}

/// Snapshots of the document before each undoable step.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichTextHistory {
    /// The documents before each step, oldest first.
    pub undo_stack: RichTextDocVec,
    /// The documents an undo replaced, the most recently undone last.
    pub redo_stack: RichTextDocVec,
    /// At most this many undo steps are kept (the oldest go first).
    pub limit: usize,
    /// The group of the newest step (typing into one block merges).
    pub group: RichEditGroup,
}

impl Default for RichTextHistory {
    fn default() -> Self {
        Self::create()
    }
}

impl RichTextHistory {
    /// An empty history keeping up to 200 steps.
    #[must_use]
    pub const fn create() -> Self {
        Self {
            undo_stack: RichTextDocVec::from_vec(Vec::new()),
            redo_stack: RichTextDocVec::from_vec(Vec::new()),
            limit: 200,
            group: RichEditGroup::None,
        }
    }

    /// Records a step: `before` is the document as it was before it. A step
    /// of the same group as the one before it (typing on in one block) adds
    /// nothing - the snapshot before the first keystroke covers them all. A
    /// new step forgets what was undone.
    pub fn record(&mut self, before: &RichTextDoc, group: RichEditGroup) {
        let merges = group != RichEditGroup::None && group == self.group;
        self.group = group;
        if merges {
            return;
        }
        let mut undo = core::mem::take(&mut self.undo_stack).into_library_owned_vec();
        undo.push(before.clone());
        let limit = self.limit.max(1);
        if undo.len() > limit {
            let excess = undo.len() - limit;
            undo = undo.split_off(excess);
        }
        self.undo_stack = RichTextDocVec::from_vec(undo);
        self.redo_stack = RichTextDocVec::from_vec(Vec::new());
    }

    /// Ends the current group: the next step is a step of its own (the
    /// caret moved, a command ran).
    pub const fn break_group(&mut self) {
        self.group = RichEditGroup::None;
    }

    /// Whether there is a step to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.as_ref().is_empty()
    }

    /// Whether there is an undone step to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.as_ref().is_empty()
    }

    /// Undoes the newest step: returns the document to show (`current`
    /// goes onto the redo list), or `None` with nothing to undo.
    pub fn undo(&mut self, current: &RichTextDoc) -> Option<RichTextDoc> {
        let mut undo = core::mem::take(&mut self.undo_stack).into_library_owned_vec();
        let previous = undo.pop();
        self.undo_stack = RichTextDocVec::from_vec(undo);
        let previous = previous?;
        let mut redo = core::mem::take(&mut self.redo_stack).into_library_owned_vec();
        redo.push(current.clone());
        self.redo_stack = RichTextDocVec::from_vec(redo);
        self.group = RichEditGroup::None;
        Some(previous)
    }

    /// Redoes the newest undone step: returns the document to show
    /// (`current` goes back onto the undo list), or `None`.
    pub fn redo(&mut self, current: &RichTextDoc) -> Option<RichTextDoc> {
        let mut redo = core::mem::take(&mut self.redo_stack).into_library_owned_vec();
        let next = redo.pop();
        self.redo_stack = RichTextDocVec::from_vec(redo);
        let next = next?;
        let mut undo = core::mem::take(&mut self.undo_stack).into_library_owned_vec();
        undo.push(current.clone());
        self.undo_stack = RichTextDocVec::from_vec(undo);
        self.group = RichEditGroup::None;
        Some(next)
    }

    /// Forgets every step (a new document was loaded).
    pub fn clear(&mut self) {
        self.undo_stack = RichTextDocVec::from_vec(Vec::new());
        self.redo_stack = RichTextDocVec::from_vec(Vec::new());
        self.group = RichEditGroup::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::rich_text::doc::RichBlock;

    fn doc(text: &str) -> RichTextDoc {
        RichTextDoc::from_blocks(vec![RichBlock::paragraph(text)])
    }

    #[test]
    fn typing_in_one_block_is_one_step_and_undo_then_redo_walk_it() {
        let mut history = RichTextHistory::create();
        history.record(&doc(""), RichEditGroup::Typing(0));
        history.record(&doc("a"), RichEditGroup::Typing(0));
        history.record(&doc("ab"), RichEditGroup::Typing(0));
        assert_eq!(history.undo_stack.as_ref().len(), 1, "one step for the typing");
        let back = history.undo(&doc("abc")).expect("a step to undo");
        assert_eq!(back, doc(""));
        assert!(!history.can_undo());
        assert!(history.can_redo());
        let again = history.redo(&back).expect("a step to redo");
        assert_eq!(again, doc("abc"));
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn another_block_or_a_command_starts_a_new_step_and_forgets_the_redo() {
        let mut history = RichTextHistory::create();
        history.record(&doc(""), RichEditGroup::Typing(0));
        history.record(&doc("a"), RichEditGroup::Typing(1));
        history.record(&doc("ab"), RichEditGroup::None);
        history.record(&doc("abc"), RichEditGroup::None);
        assert_eq!(history.undo_stack.as_ref().len(), 4);
        let _ = history.undo(&doc("abcd"));
        assert!(history.can_redo());
        history.record(&doc("abc"), RichEditGroup::None);
        assert!(!history.can_redo(), "a new step forgets the undone ones");
        history.record(&doc("x"), RichEditGroup::Typing(0));
        history.break_group();
        history.record(&doc("xy"), RichEditGroup::Typing(0));
        assert_eq!(
            history.undo_stack.as_ref().len(),
            6,
            "a broken group starts a new step"
        );
    }

    #[test]
    fn the_history_keeps_at_most_its_limit() {
        let mut history = RichTextHistory::create();
        history.limit = 3;
        for i in 0..5 {
            history.record(&doc(&alloc::format!("{i}")), RichEditGroup::None);
        }
        assert_eq!(history.undo_stack.as_ref().len(), 3);
        assert_eq!(history.undo_stack.as_ref()[0], doc("2"), "the oldest went first");
        assert_eq!(history.undo(&doc("5")), Some(doc("4")));
        assert_eq!(RichTextHistory::create().undo(&doc("a")), None);
    }
}
