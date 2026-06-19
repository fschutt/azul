//! Windows clipboard integration
//!
//! Uses the `clipboard-win` crate to interface with the Windows clipboard API.
//!
//! - [`write_to_clipboard`]: writes a string to the system clipboard.
//! - [`get_clipboard_content`]: reads the current text content from the system clipboard.
//!
//! Both are called from `common/event.rs` during event processing (via the
//! `get_system_clipboard`/`set_system_clipboard` helpers).

/// Write text to the Windows system clipboard
pub fn write_to_clipboard(text: &str) -> Result<(), ()> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).map_err(|_| ())
}

/// Read content from Windows system clipboard
///
/// Returns the clipboard text content if available.
pub fn get_clipboard_content() -> Option<String> {
    use clipboard_win::{formats, get_clipboard};

    get_clipboard::<String, _>(formats::Unicode).ok()
}
