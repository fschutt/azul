//! The window's one dialog, and the dialogs waiting for it.
//!
//! Every dialog AzDrive shows goes through [`PopupSlot`]. The app opens and closes dialogs as its
//! work goes (a question, a progress, an answer); the user closes them with their close box,
//! Escape or their own buttons. A dialog that shows something only this once (a new drive's
//! recovery code, a join code, the shares made for trusted contacts) must be seen: nothing the
//! app does replaces or closes it, a dialog opened meanwhile waits behind it, and only its user's
//! own close takes it away. That holds however late a background answer arrives, so no timing of
//! jobs and rebuilds can take such a dialog away before it was read.
//!
//! The dialog's node in the page carries one fixed id ([`crate::ids::POPUP`]), so the window
//! it shows in is the slot's: a dialog that turns into another (a busy setup into its recovery
//! sheet) keeps its window whatever else the same rebuild changed around it, and a dialog opened
//! after the slot was empty for a frame gets a window of its own.

use crate::Popup;

/// The dialog showing and the ones waiting for it.
#[derive(Default)]
pub(crate) struct PopupSlot {
    shown: Option<Popup>,
    /// Dialogs that could not show yet, oldest first: they show one by one once no dialog does.
    waiting: Vec<Popup>,
}

impl PopupSlot {
    /// The dialog showing.
    pub(crate) const fn as_ref(&self) -> Option<&Popup> {
        self.shown.as_ref()
    }

    /// The dialog showing, to change what it holds (what was typed, its note).
    pub(crate) const fn as_mut(&mut self) -> Option<&mut Popup> {
        self.shown.as_mut()
    }

    pub(crate) const fn is_none(&self) -> bool {
        self.shown.is_none()
    }

    pub(crate) const fn is_some(&self) -> bool {
        self.shown.is_some()
    }

    /// The dialogs waiting to show.
    pub(crate) fn waiting(&self) -> impl Iterator<Item = &Popup> {
        self.waiting.iter()
    }

    /// Shows `popup` in place of the dialog showing. A dialog that must be seen is not replaced:
    /// `popup` waits behind it, unless it only reports progress (that work answers on its own,
    /// and a progress shown after its work ended would never close).
    pub(crate) fn open(&mut self, popup: Popup) {
        match &self.shown {
            Some(shown) if shown.must_be_seen() => {
                if !popup.reports_progress() {
                    self.waiting.push(popup);
                }
            }
            _ => self.shown = Some(popup),
        }
    }

    /// Shows `popup` once no other dialog shows: at once in place of a progress dialog (the
    /// answer of its work), else after the dialog showing has closed.
    pub(crate) fn open_when_free(&mut self, popup: Popup) {
        match &self.shown {
            Some(shown) if !shown.reports_progress() => self.waiting.push(popup),
            _ => self.shown = Some(popup),
        }
    }

    /// The app is done with the dialog showing (its work ended, its answer was taken): it
    /// closes, unless it must be seen - only its user closes that.
    pub(crate) fn close(&mut self) {
        if !self.shown.as_ref().is_some_and(Popup::must_be_seen) {
            self.shown = None;
        }
    }

    /// Takes the dialog showing to act on its answer, leaving none. A dialog that must be seen
    /// is not taken: the answer meant for another dialog finds none.
    pub(crate) fn take(&mut self) -> Option<Popup> {
        if self.shown.as_ref().is_some_and(Popup::must_be_seen) {
            None
        } else {
            self.shown.take()
        }
    }

    /// The user closed the dialog showing (its close box, Escape, its Done or Close button):
    /// it goes, whatever it is.
    pub(crate) fn user_closed(&mut self) {
        self.shown = None;
    }

    /// Shows the oldest waiting dialog when none shows. Returns whether one showed. Run after
    /// every callback and every job's answer, so a dialog never waits longer than the one before
    /// it.
    pub(crate) fn show_waiting(&mut self) -> bool {
        if self.shown.is_some() || self.waiting.is_empty() {
            return false;
        }
        self.shown = Some(self.waiting.remove(0));
        true
    }
}
