//! Improved cross-platform clipboard library
//!
//! Fork of https://github.com/aweinstock314/rust-clipboard with better error handling

#[cfg(target_os = "windows")]
extern crate clipboard_win;
#[cfg(any(target_os = "linux", target_os = "openbsd"))]
extern crate x11_clipboard;
#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;
#[cfg(target_os = "macos")]
extern crate objc_foundation;
#[cfg(target_os = "macos")]
extern crate objc_id;

pub mod clipboard_metadata;
mod errors;

pub use clipboard_metadata::ClipboardContentType;
pub use errors::ClipboardError;

pub trait Clipboard {
    type Output;
    fn new() -> Result<Self::Output, ClipboardError>;
    fn get_contents(&self) -> Result<(Vec<u8>, ClipboardContentType), ClipboardError>;
    fn get_string_contents(&self) -> Result<String, ClipboardError>;
    fn set_contents(
        &self,
        contents: Vec<u8>,
        format: ClipboardContentType,
    ) -> Result<(), ClipboardError>;
    fn set_string_contents(&self, contents: String) -> Result<(), ClipboardError>;
}

#[cfg(target_os = "windows")]
pub mod win;
#[cfg(target_os = "windows")]
pub use win::WindowsClipboard as SystemClipboard;

#[cfg(any(target_os = "linux", target_os = "openbsd"))]
pub mod x11;
#[cfg(any(target_os = "linux", target_os = "openbsd"))]
pub use x11::X11Clipboard as SystemClipboard;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacOsClipboard as SystemClipboard;
