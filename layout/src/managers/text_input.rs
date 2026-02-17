//! Text Input Manager
//!
//! Centralizes all text editing logic for contenteditable nodes.
//!
//! This manager handles text input from multiple sources:
//!
//! - Keyboard input (character insertion, backspace, etc.)
//! - IME composition (multi-character input for Asian languages)
//! - Accessibility actions (screen readers, voice control)
//! - Programmatic edits (from callbacks)
//!
//! ## Architecture
//!
//! The text input system uses a two-phase approach:
//!
//! 1. **Record Phase**: When text input occurs, record what changed (old_text + inserted_text)
//!
//!    - Store in `pending_changeset`
//!    - Do NOT modify any caches yet
//!    - Return affected nodes so callbacks can be invoked
//!
//! 2. **Apply Phase**: After callbacks, if preventDefault was not set:
//!
//!    - Compute new text using text3::edit
//!    - Update cursor position
//!    - Update text cache
//!    - Mark nodes dirty for re-layout
//!
//! This separation allows:
//!
//! - User callbacks to inspect the changeset before it's applied
//! - preventDefault to cancel the edit
//! - Consistent behavior across keyboard/IME/A11y sources

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    events::{EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent},
    selection::TextCursor,
    task::Instant,
};
use azul_css::corety::AzString;

/// Information about a pending text edit that hasn't been applied yet
#[derive(Debug, Clone)]
#[repr(C)]
pub struct PendingTextEdit {
    /// The node that was edited
    pub node: DomNodeId,
    /// The text that was inserted
    pub inserted_text: AzString,
    /// The old text before the edit (plain text extracted from InlineContent)
    pub old_text: AzString,
}

impl PendingTextEdit {
    /// Compute the resulting text after applying the edit
    ///
    /// This is a pure function that applies the inserted_text to old_text
    /// using the current cursor position.
    ///
    /// NOTE: Actual text application is handled by apply_text_changeset() in window.rs
    /// which uses text3::edit::insert_text() for proper cursor-based insertion.
    /// This method is for preview/inspection purposes only.
    pub fn resulting_text(&self, cursor: Option<&TextCursor>) -> AzString {
        // For preview: append the inserted text
        // Actual insertion at cursor is done by text3::edit::insert_text()
        let mut result = self.old_text.as_str().to_string();
        result.push_str(self.inserted_text.as_str());

        let _ = cursor; // Preview doesn't need cursor - actual insert does

        result.into()
    }
}

/// C-compatible Option type for PendingTextEdit
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum OptionPendingTextEdit {
    None,
    Some(PendingTextEdit),
}

impl OptionPendingTextEdit {
    pub fn into_option(self) -> Option<PendingTextEdit> {
        match self {
            OptionPendingTextEdit::None => None,
            OptionPendingTextEdit::Some(t) => Some(t),
        }
    }
}

impl From<Option<PendingTextEdit>> for OptionPendingTextEdit {
    fn from(o: Option<PendingTextEdit>) -> Self {
        match o {
            Some(v) => OptionPendingTextEdit::Some(v),
            None => OptionPendingTextEdit::None,
        }
    }
}

impl<'a> From<Option<&'a PendingTextEdit>> for OptionPendingTextEdit {
    fn from(o: Option<&'a PendingTextEdit>) -> Self {
        match o {
            Some(v) => OptionPendingTextEdit::Some(v.clone()),
            None => OptionPendingTextEdit::None,
        }
    }
}

/// Source of a text input event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputSource {
    /// Regular keyboard input
    Keyboard,
    /// IME composition (multi-character input)
    Ime,
    /// Accessibility action from assistive technology
    Accessibility,
    /// Programmatic edit from user callback
    Programmatic,
}

/// Text Input Manager
///
/// Centralizes all text editing logic. This is the single source of truth
/// for text input state.
pub struct TextInputManager {
    /// The pending text changeset that hasn't been applied yet.
    /// This is set during the "record" phase and cleared after the "apply" phase.
    pub pending_changeset: Option<PendingTextEdit>,
    /// Source of the current text input
    pub input_source: Option<TextInputSource>,
}

impl TextInputManager {
    /// Create a new TextInputManager
    pub fn new() -> Self {
        Self {
            pending_changeset: None,
            input_source: None,
        }
    }

    /// Record a text input event (Phase 1)
    ///
    /// This ONLY records what text was inserted. It does NOT apply the changes yet.
    /// The changes are applied later in `apply_changeset()` if preventDefault is not set.
    ///
    /// # Arguments
    ///
    /// - `node` - The DOM node being edited
    /// - `inserted_text` - The text being inserted
    /// - `old_text` - The current text before the edit
    /// - `source` - Where the input came from (keyboard, IME, A11y, etc.)
    ///
    /// Returns the affected node for event generation.
    pub fn record_input(
        &mut self,
        node: DomNodeId,
        inserted_text: String,
        old_text: String,
        source: TextInputSource,
    ) -> DomNodeId {
        // Clear any previous changeset
        self.pending_changeset = None;

        // Store the new changeset
        self.pending_changeset = Some(PendingTextEdit {
            node,
            inserted_text: inserted_text.into(),
            old_text: old_text.into(),
        });

        self.input_source = Some(source);

        node
    }

    /// Get the pending changeset (if any)
    pub fn get_pending_changeset(&self) -> Option<&PendingTextEdit> {
        self.pending_changeset.as_ref()
    }

    /// Clear the pending changeset
    ///
    /// This is called after applying the changeset or if preventDefault was set.
    pub fn clear_changeset(&mut self) {
        self.pending_changeset = None;
        self.input_source = None;
    }

    /// Check if there's a pending changeset that needs to be applied
    pub fn has_pending_changeset(&self) -> bool {
        self.pending_changeset.is_some()
    }
}

impl Default for TextInputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EventProvider for TextInputManager {
    /// Get pending text input events.
    ///
    /// If there's a pending changeset, returns an Input event for the affected node.
    /// The event data includes the old text and inserted text so callbacks can
    /// query the changeset.
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();

        if let Some(changeset) = &self.pending_changeset {
            let event_source = match self.input_source {
                Some(TextInputSource::Keyboard) | Some(TextInputSource::Ime) => {
                    CoreEventSource::User
                }
                Some(TextInputSource::Accessibility) => CoreEventSource::User, /* A11y is still */
                // user input
                Some(TextInputSource::Programmatic) => CoreEventSource::Programmatic,
                None => CoreEventSource::User,
            };

            // Generate Input event (fires on every keystroke)
            events.push(SyntheticEvent::new(
                EventType::Input,
                event_source,
                changeset.node,
                timestamp,
                // Callbacks can query changeset via
                // text_input_manager.get_pending_changeset()
                EventData::None,
            ));

            // Note: We don't generate Change events here - those are generated
            // when focus is lost or Enter is pressed (handled elsewhere)
        }

        events
    }
}
