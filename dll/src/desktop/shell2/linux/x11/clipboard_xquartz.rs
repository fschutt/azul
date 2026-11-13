//! The X11 backend's clipboard on a macOS host (`x11-macos`, XQuartz).
//!
//! Same API as `clipboard.rs`, different transport. On Linux the X11 window
//! owns and reads the X selections itself through the `x11-clipboard` crate.
//! That crate is pure x11rb and would build on macOS, but it cannot be
//! brought into ONLY the `x11-macos` build: `_internal_deps` enables the
//! optional dependency named `x11-clipboard`, so declaring that name for macOS
//! would compile it into every macOS build, and declaring the same package
//! under a second name is refused by cargo ("depends on crate `x11-clipboard`
//! multiple times with different names" - cargo resolves one package to one
//! crate name per dependent, across target tables).
//!
//! So on a Mac the X11 window speaks to the pasteboard, and XQuartz does the
//! rest: its pasteboard proxy mirrors CLIPBOARD to and from `NSPasteboard`
//! (XQuartz > Settings > Pasteboard, on by default), so other X clients and
//! macOS apps see a copy either way. PRIMARY - the select-then-middle-click
//! idiom - stays inside this process: selecting text and middle-clicking
//! within the app works, other X clients do not see it.
//!
//! What this means for reproducing bugs: everything up to the transport - the
//! selection gestures, the Ctrl+C / Ctrl+V / middle-click paths through
//! `common/event.rs` and `x11/events.rs` - is the Linux code. The worker
//! thread, its deadlines and the multi-target probe of `clipboard.rs` are not
//! exercised here; those still need a Linux X server.

use std::sync::Mutex;

use rich_clipboard::{decode_payload, encode, ClipboardPayload, Platform, RichItem};

/// The text most recently put on PRIMARY by this process. The only PRIMARY
/// there is off Linux - see the module docs.
static LAST_PRIMARY: Mutex<Option<String>> = Mutex::new(None);

/// Write text to the clipboard (CLIPBOARD, through the pasteboard) and to
/// PRIMARY, as the X11 transport does.
pub fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    write_to_primary(text)?;
    let payload = encode(&RichItem::Text(text.to_owned()), Platform::MacOs)
        .map_err(|_| ClipboardError::WriteFailed)?;
    if crate::desktop::shell2::macos::clipboard::write_payload(&payload) {
        Ok(())
    } else {
        Err(ClipboardError::WriteFailed)
    }
}

/// Claim PRIMARY with this text (the X11 select-to-copy idiom) - in-process
/// only on this host.
pub fn write_to_primary(text: &str) -> Result<(), ClipboardError> {
    let mut primary = LAST_PRIMARY
        .lock()
        .map_err(|_| ClipboardError::WriteFailed)?;
    *primary = Some(text.to_owned());
    Ok(())
}

/// Every flavor the clipboard offers, as the pasteboard reports them.
pub fn read_payload() -> Option<ClipboardPayload> {
    crate::desktop::shell2::macos::clipboard::read_payload()
}

/// The clipboard as plain text, through the full decode policy (richest
/// flavor first), so an RTF- or HTML-only clipboard still pastes as text.
pub fn get_clipboard_content() -> Option<String> {
    let payload = read_payload()?;
    let item = decode_payload(&payload).ok()?;
    item.plain_text().map(str::to_owned)
}

/// PRIMARY - the middle-click paste source: the text this process last
/// selected.
pub fn get_primary_content() -> Option<String> {
    LAST_PRIMARY.lock().ok().and_then(|g| g.clone())
}

#[derive(Debug, Copy, Clone)]
pub enum ClipboardError {
    InitFailed,
    WriteFailed,
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardError::InitFailed => write!(f, "failed to initialize the clipboard"),
            ClipboardError::WriteFailed => write!(f, "failed to write to the clipboard"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_primary_content, write_to_primary};

    /// PRIMARY never leaves the process on this host, so a selection
    /// followed by a middle click must round-trip through the parked copy
    /// alone - there is no X selection to fall back on.
    #[test]
    fn a_selection_is_what_a_middle_click_pastes() {
        write_to_primary("selected words").expect("parking PRIMARY cannot fail");
        assert_eq!(get_primary_content().as_deref(), Some("selected words"));
    }
}
