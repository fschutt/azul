//! libgtk-3's input method context, for the X11 and the Wayland shell.
//!
//! The shells hand the caret rectangle of a focused text field to it
//! (`gtk_im_context_set_cursor_location`) when they have no better route (X11: no XIM; Wayland: after the
//! text-input v3 protocol). libgtk-3 pulls in GDK, Pango, Cairo, GIO and about a hundred more libraries, so a window
//! must NOT load it when it is created (it was, on every start, and most windows never ask a context for anything):
//! each shell keeps a `OnceCell<Option<GtkImFallback>>` and fills it at the first caret position it has to hand
//! over ([`load`]). The startup timeline (`AZ_STARTUP=1`) marks the load as `gtk_im_loaded`.

use std::rc::Rc;

use super::x11::dlopen::{Gtk3Im, GtkIMContext};
use crate::desktop::startup_timeline::StartupTimeline;

/// libgtk-3 and the input method context made from it.
pub struct GtkImFallback {
    pub gtk: Rc<Gtk3Im>,
    pub context: *mut GtkIMContext,
}

/// Loads libgtk-3 and makes its IM context: `None` when the library or the context is not there (the candidate
/// window of an input method then stays where the input method puts it). Called at most once per window, by the
/// first caret position that is handed over. `shell` names the caller in the log and in the timeline.
pub fn load(startup: &StartupTimeline, shell: &str) -> Option<GtkImFallback> {
    let loaded = match Gtk3Im::new() {
        Ok(gtk) => {
            let context = unsafe { (gtk.gtk_im_context_simple_new)() };
            if context.is_null() {
                crate::plog_warn!("[{}] Failed to create GTK IM context instance", shell);
                None
            } else {
                crate::plog_info!("[{}] GTK3 IM context loaded for IME support", shell);
                Some(GtkImFallback { gtk, context })
            }
        }
        Err(e) => {
            crate::plog_debug!(
                "[{}] GTK3 IM not available (IME positioning disabled): {:?}",
                shell,
                e
            );
            None
        }
    };
    startup.mark_with("gtk_im_loaded", || {
        format!("{shell} loaded={}", loaded.is_some())
    });
    loaded
}

#[cfg(test)]
#[path = "gtk_im_test.rs"]
mod gtk_im_test;
