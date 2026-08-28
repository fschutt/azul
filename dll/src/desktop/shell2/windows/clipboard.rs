//! Windows clipboard transport: the Win32 clipboard ⇄ [`ClipboardPayload`].
//!
//! Transport only — this module knows about format numbers, `GlobalSize` and
//! handle lifetimes, and nothing about what any of those bytes mean. The
//! formats live in `rich-clipboard`, reached through
//! `shell2/common/clipboard.rs`.
//!
//! Built on `clipboard-win`'s `raw` layer rather than its typed one: the typed
//! `get_clipboard::<String, _>(formats::Unicode)` reaches exactly one format,
//! which is the whole thing a payload transport must not do.
//!
//! # What is easy to get wrong here
//!
//! * **The clipboard is a global lock held by one process at a time.** Every
//!   read and write goes through [`with_clipboard`], which retries the open —
//!   another application holding it for a few milliseconds is routine, not an
//!   error — and closes it on the way out even if the body panicked, because
//!   `Clipboard`'s `Drop` does that.
//! * **`set` empties the clipboard first.** Publishing a fan-out of four
//!   formats with it would leave only the last one. The write empties *once*
//!   and then uses `set_without_clear` per format.
//! * **Predefined formats have no name.** `GetClipboardFormatNameW` returns
//!   nothing for `CF_DIB` and friends, so the number is mapped through
//!   [`WindowsFormat::name`] — which is exactly what `Flavor::from_windows_name`
//!   reads back — and only registered formats are asked for their string.
//! * **`CF_BITMAP` and `CF_ENHMETAFILE` are handles, not bytes.** `GlobalSize`
//!   on an `HBITMAP` is meaningless. They are skipped rather than dumped as
//!   whatever bytes happen to be at that address.
//! * **Ask the size before copying.** `GlobalSize` on the handle is the
//!   `SizeHint::Exact` this platform can state for free, before any copy — so
//!   an oversize flavor costs nothing and the decode falls through to the
//!   next-best one.
//!
//! **Never run against a real Windows clipboard.** This is written from the
//! Win32 documentation and `clipboard-win`'s source; treat the first run as a
//! debugging session, not a smoke test.

use clipboard_win::{formats, raw, Clipboard};
use rich_clipboard::{ClipboardItem, ClipboardPayload, Flavor, Platform};

use super::super::common::clipboard::MAX_FLAVOR_BYTES;
use super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_warn};

/// How many times to retry `OpenClipboard` before giving up.
///
/// The clipboard is a global lock: another application repainting a paste
/// preview holds it for a few milliseconds and `OpenClipboard` fails outright
/// rather than blocking. `clipboard-win` sleeps between attempts, so this is
/// tens of milliseconds worst case — bounded, because this runs on the UI
/// thread and a wedged clipboard owner must cost a paste, not the frame loop.
const OPEN_ATTEMPTS: usize = 10;

/// Formats whose clipboard "data" is a HANDLE rather than a memory block.
///
/// `GlobalSize` on an `HBITMAP` or `HENHMETAFILE` does not describe the
/// object, so reading them as bytes produces garbage of an arbitrary length.
/// A consumer that wants these needs `GetDIBits` / `GetEnhMetaFileBits`, which
/// is a real conversion and not a transport concern.
const HANDLE_FORMATS: &[u32] = &[
    formats::CF_BITMAP,
    formats::CF_ENHMETAFILE,
    formats::CF_METAFILEPICT,
    formats::CF_PALETTE,
    formats::CF_OWNERDISPLAY,
];

/// Run `f` with the clipboard open, closing it afterwards.
fn with_clipboard<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    // `Clipboard`'s Drop calls CloseClipboard, so an early return inside `f`
    // cannot leak the global lock.
    let _guard = Clipboard::new_attempts(OPEN_ATTEMPTS).ok()?;
    f()
}

/// Read every format currently on the clipboard.
///
/// `None` for an empty or unreachable clipboard.
pub fn read_payload() -> Option<ClipboardPayload> {
    with_clipboard(|| {
        let mut payload = ClipboardPayload::new(Platform::Windows);
        // Windows has one selection with many formats and no notion of items,
        // so everything belongs to item 0.
        for format in raw::EnumFormats::new() {
            if HANDLE_FORMATS.contains(&format) {
                log_debug!(
                    LogCategory::Resources,
                    "[Windows] skipping clipboard format {format}: a handle, not bytes"
                );
                continue;
            }
            let Some(native) = format_name(format) else {
                continue;
            };
            // GlobalSize BEFORE the copy — the whole reason the read is split
            // in two steps.
            let size = raw::size(format).map_or(0, |s| s.get() as u64);
            if size > MAX_FLAVOR_BYTES {
                log_warn!(
                    LogCategory::Resources,
                    "[Windows] skipping clipboard format `{native}`: {size} bytes exceeds the \
                     {MAX_FLAVOR_BYTES}-byte cap"
                );
                continue;
            }

            let mut bytes = Vec::new();
            if raw::get_vec(format, &mut bytes).is_err() || bytes.is_empty() {
                // A format that is advertised but not readable — a delayed
                // render whose owner declined. Normal; skip it.
                continue;
            }
            payload.push(ClipboardItem::new(native, bytes));
        }
        (!payload.is_empty()).then_some(payload)
    })
}

/// The identifier `Flavor::from_windows_name` reads back.
///
/// A predefined format has no name from `GetClipboardFormatNameW`, so its
/// canonical `CF_*` spelling comes from the registry; only a registered format
/// is asked for its string. `None` for a predefined format the registry does
/// not name — those are formats nothing in this stack can decode anyway
/// (`CF_SYLK`, `CF_PENDATA`, the private ranges).
fn format_name(format: u32) -> Option<String> {
    use rclip_core::flavor::WindowsFormat;

    if let Some(name) = WindowsFormat::Predefined(format).name() {
        return Some(name.to_owned());
    }
    // Predefined numbers below CF_MAX that the registry does not name are not
    // registered formats either — asking Windows for their name returns
    // nothing and inventing one would produce an identifier no reader knows.
    raw::format_name_big(format)
}

/// Publish every flavor of a payload to the clipboard.
///
/// `false` unless at least one format was accepted: `CutToClipboard` gates the
/// deletion of the user's selected text on this.
pub fn write_payload(payload: &ClipboardPayload) -> bool {
    if payload.is_empty() {
        return false;
    }
    with_clipboard(|| {
        // ONCE, before the loop: `raw::set` would empty before every format
        // and leave only the last one on the clipboard.
        if raw::empty().is_err() {
            return None;
        }
        let mut written = 0usize;
        for entry in payload.items() {
            let Some(format) = format_id(&entry.native) else {
                continue;
            };
            if raw::set_without_clear(format, &entry.bytes).is_ok() {
                written += 1;
            }
        }
        (written > 0).then_some(())
    })
    .is_some()
}

/// The Win32 format number for a payload identifier.
///
/// The reverse of [`format_name`]: a predefined format resolves through the
/// registry to its number, and everything else is registered by name (which
/// returns the existing id when the format is already registered — that is
/// what makes `RegisterClipboardFormat` idempotent).
fn format_id(native: &str) -> Option<u32> {
    use rclip_core::flavor::WindowsFormat;

    match Flavor::from_windows_name(native).windows() {
        Some(WindowsFormat::Predefined(id)) => Some(id),
        Some(WindowsFormat::Registered(name)) => raw::register_format(name).map(|id| id.get()),
        // An identifier the registry does not know — a private format carried
        // through verbatim by `RichItem::Unknown`. Register it under the name
        // it arrived with.
        None => raw::register_format(native).map(|id| id.get()),
    }
}

/// Write text to the Windows system clipboard.
pub fn write_to_clipboard(text: &str) -> Result<(), ()> {
    let payload = rich_clipboard::encode(
        &rich_clipboard::RichItem::Text(text.to_owned()),
        Platform::Windows,
    )
    .map_err(|_| ())?;
    if write_payload(&payload) {
        Ok(())
    } else {
        Err(())
    }
}

/// Read the clipboard's text content.
///
/// Goes through the payload path, so a clipboard offering only RTF or `CF_HTML`
/// still answers — the old `get_clipboard::<String, _>(formats::Unicode)` came
/// back empty for those.
pub fn get_clipboard_content() -> Option<String> {
    let payload = read_payload()?;
    rich_clipboard::decode_payload(&payload)
        .ok()?
        .plain_text()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name a predefined format is published under has to be the one
    /// `Flavor::from_windows_name` reads back, or a payload written by this
    /// module would not resolve when read by it.
    ///
    /// NEGATIVE CONTROL: make `format_name` call `format_name_big` first —
    /// `CF_DIB` comes back as `None` and the flavor is lost.
    #[test]
    fn predefined_format_names_round_trip_through_the_registry() {
        for (id, flavor) in [
            (formats::CF_UNICODETEXT, Flavor::PlainText),
            (formats::CF_DIB, Flavor::Dib),
            (formats::CF_DIBV5, Flavor::DibV5),
            (formats::CF_HDROP, Flavor::FileList),
        ] {
            let name = super::format_name(id).expect("a predefined format must have a name");
            assert_eq!(
                Flavor::from_windows_name(&name),
                flavor,
                "{name} did not resolve back to the flavor it names"
            );
        }
    }

    /// A handle format must never be read as bytes: `GlobalSize` on an
    /// `HBITMAP` describes nothing, so whatever came back would be garbage of
    /// an arbitrary length.
    #[test]
    fn handle_formats_are_not_byte_formats() {
        assert!(HANDLE_FORMATS.contains(&formats::CF_BITMAP));
        assert!(HANDLE_FORMATS.contains(&formats::CF_ENHMETAFILE));
        // CF_DIB *is* bytes and must not be in the list — it is the format
        // rclip-dib decodes.
        assert!(!HANDLE_FORMATS.contains(&formats::CF_DIB));
    }
}
