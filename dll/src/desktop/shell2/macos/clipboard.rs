//! macOS clipboard transport: `NSPasteboard` ⇄ [`ClipboardPayload`].
//!
//! Transport only — this module knows about pasteboard items, types and byte
//! counts, and nothing about what any of those bytes mean. The formats live in
//! `rich-clipboard`, reached through `shell2/common/clipboard.rs`.
//!
//! Written against `objc2-app-kit`, whose `generalPasteboard` / `types` /
//! `pasteboardItems` / `dataForType:` bindings are all safe, so there is no
//! `unsafe` here at all. (The `objc` 0.2 version this replaced needed a
//! `transmute` of a `&Class` to get `readObjectsForClasses:` to typecheck.)
//!
//! # Four things about `NSPasteboard` that are easy to get wrong
//!
//! * **A pasteboard holds *items*, not formats.** `-[NSPasteboard types]` is
//!   the union of every item's types and `-[NSPasteboard dataForType:]` only
//!   reaches the *first* item offering that type. Copy three files in Finder
//!   and the pasteboard holds three items each carrying one `public.file-url`;
//!   the pasteboard-level API shows one URL and silently drops the other two.
//!   [`read_payload`] therefore walks `-pasteboardItems` and records which item
//!   each representation came from.
//! * **Types are promises.** `dataForType:` returns nil for a type that is
//!   genuinely on offer when the owning application declared it lazily and then
//!   declined — Safari advertises `com.apple.linkpresentation.metadata` and
//!   never provides it. Skipped, not an error.
//! * **Every modern UTI has a byte-identical legacy twin** (`public.rtf` and
//!   `NeXT Rich Text Format v1.0 pasteboard type`, and six more). The registry
//!   resolves both to one `Flavor`, so a payload carrying both would decode
//!   everything twice — [`read_payload`] drops the second spelling.
//! * **Ask the size before copying.** The bytes are already resident and owned
//!   by the pasteboard server, so `-[NSData length]` costs nothing and happens
//!   *before* `to_vec()`. A flavor past the cap is skipped and the decode falls
//!   through to the next-best one.

use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem};
use objc2_foundation::{NSArray, NSData, NSString};
use rich_clipboard::{ClipboardItem, ClipboardPayload, Flavor, Platform};

use super::super::common::clipboard::MAX_FLAVOR_BYTES;
use super::super::common::debug_server::LogCategory;
use crate::log_warn;

/// Read every flavor on the general pasteboard.
///
/// `None` for an empty or unreachable pasteboard. An empty payload is reported
/// as `None` too: "the clipboard offered nothing we could read" and "there is
/// no clipboard" are the same thing to a paste.
pub fn read_payload() -> Option<ClipboardPayload> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let mut payload = ClipboardPayload::new(Platform::MacOs);

    // Items, not the pasteboard-level API: see the module docs. Walked even
    // for a single item, so a one-file copy and a three-file copy take the
    // same code path rather than differing in a way only multi-select
    // exercises.
    let items = pasteboard.pasteboardItems()?;
    for (index, item) in items.iter().enumerate() {
        read_item(&item, index, &mut payload);
    }

    (!payload.is_empty()).then_some(payload)
}

/// Every representation of one pasteboard item, deduped by resolved flavor.
fn read_item(item: &NSPasteboardItem, index: usize, payload: &mut ClipboardPayload) {
    // Which flavors this ITEM has already contributed. Per-item, not
    // per-payload: three Finder items each offering `public.file-url` are
    // three files and all three must survive — it is one item offering the
    // same flavor under two spellings that is the duplicate.
    let mut seen: Vec<String> = Vec::new();

    for ty in item.types().iter() {
        let native = ty.to_string();
        // `Flavor::Other` carries the native name, so two unrecognised types
        // never collide with each other; two spellings of a known flavor do.
        let flavor = Flavor::from_uti(&native);
        let key = flavor_key(flavor, &native);
        if seen.iter().any(|s| *s == key) {
            continue;
        }

        let Some(data) = item.dataForType(&ty) else {
            // A promise the owner declined. Normal — do not warn.
            continue;
        };
        let Some(bytes) = take_bytes(&data, &native) else {
            continue;
        };

        seen.push(key);
        payload.push(ClipboardItem::in_item(index, native, bytes));
    }
}

/// A stable identity for "this flavor", for the dedupe.
fn flavor_key(flavor: Flavor<'_>, native: &str) -> String {
    match flavor {
        // Unrecognised: the native name IS the identity.
        Flavor::Other(_) => format!("other:{native}"),
        known => format!("{known:?}"),
    }
}

/// Copy an `NSData` out, refusing one that is over the cap.
///
/// The length check is the whole point of doing this here rather than at
/// `to_vec()`: on macOS the size is knowable exactly and for free before any
/// copy happens, so an oversize flavor costs nothing at all. Skipping it lets
/// the decode fall through to the next-best flavor — a 400 MB TIFF goes and
/// the plain text alongside it stays.
fn take_bytes(data: &NSData, native: &str) -> Option<Vec<u8>> {
    let len = data.len() as u64;
    if len > MAX_FLAVOR_BYTES {
        log_warn!(
            LogCategory::Resources,
            "[macOS] skipping pasteboard flavor `{native}`: {len} bytes exceeds the {MAX_FLAVOR_BYTES}-byte cap"
        );
        return None;
    }
    Some(data.to_vec())
}

/// Publish a payload to the general pasteboard.
///
/// One `NSPasteboardItem` per [`ClipboardItem::item`] index, which is the only
/// way to say "these four files" on a pasteboard — an item advertising
/// `public.file-url` four times is one file advertised four times, and a reader
/// using `-[NSPasteboard dataForType:]` would see the first.
///
/// `false` unless the pasteboard accepted at least one item: `CutToClipboard`
/// gates the deletion of the user's selected text on this.
pub fn write_payload(payload: &ClipboardPayload) -> bool {
    if payload.is_empty() {
        return false;
    }

    // Group the representations by pasteboard item, preserving the order
    // within each — `encode` emits best-flavor-first and a pasteboard reader
    // is entitled to take the first type it recognises.
    let mut items: Vec<Vec<&ClipboardItem>> = Vec::new();
    for entry in payload.items() {
        if items.len() <= entry.item {
            items.resize_with(entry.item + 1, Vec::new);
        }
        items[entry.item].push(entry);
    }

    // Build every item first, then publish them in ONE `writeObjects:`. That
    // is the documented usage and it does not depend on whether repeated
    // calls append to or replace the pasteboard's items — a distinction the
    // bindings do not state and which decides whether a three-file copy ends
    // up as three files or as one.
    let mut objects = Vec::with_capacity(items.len());
    for group in &items {
        let item = NSPasteboardItem::new();
        let mut any = false;
        for entry in group {
            let ty = NSString::from_str(&entry.native);
            let data = NSData::with_bytes(&entry.bytes);
            if item.setData_forType(&data, &ty) {
                any = true;
            }
        }
        // An item nothing could be set on would advertise no types at all.
        if any {
            objects.push(ProtocolObject::from_retained(item));
        }
    }
    if objects.is_empty() {
        log_warn!(
            LogCategory::Resources,
            "[macOS] no representation of a {}-flavor payload could be set on a pasteboard item",
            payload.len()
        );
        return false;
    }

    let pasteboard = NSPasteboard::generalPasteboard();
    // `clearContents` invalidates the previous owner and bumps the change
    // count; nothing may be written before it.
    pasteboard.clearContents();
    let written = pasteboard.writeObjects(&NSArray::from_retained_slice(&objects));
    if !written {
        log_warn!(
            LogCategory::Resources,
            "[macOS] the pasteboard rejected a {}-item write",
            objects.len()
        );
    }
    written
}

/// Read the pasteboard as plain text.
///
/// Kept for the callers that only ever wanted a string (the debug server, the
/// E2E harness). Goes through the payload path so a pasteboard offering only
/// RTF or HTML still answers — the old `readObjectsForClasses:[NSString]`
/// implementation returned nil for those.
pub fn get_clipboard_content() -> Option<String> {
    let payload = read_payload()?;
    rich_clipboard::decode_payload(&payload)
        .ok()?
        .plain_text()
        .map(str::to_owned)
}

/// Write a plain string to the pasteboard.
pub fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    let payload = rich_clipboard::encode(
        &rich_clipboard::RichItem::Text(text.to_owned()),
        Platform::MacOs,
    )
    .map_err(|_| ClipboardError::WriteError)?;
    if write_payload(&payload) {
        Ok(())
    } else {
        Err(ClipboardError::WriteError)
    }
}

/// Errors that can occur during macOS clipboard operations.
#[derive(Debug, Copy, Clone)]
pub enum ClipboardError {
    /// The pasteboard refused every representation of the payload.
    WriteError,
}

impl core::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WriteError => write!(f, "the macOS pasteboard rejected the write"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two spellings of ONE flavor on one item are one flavor. Every modern
    /// UTI has a byte-identical legacy twin on a real pasteboard, and carrying
    /// both means decoding the same bytes twice.
    ///
    /// NEGATIVE CONTROL: drop the `seen` check in `read_item`.
    #[test]
    fn legacy_uti_twins_share_one_key() {
        let modern = Flavor::from_uti("public.rtf");
        let legacy = Flavor::from_uti("NeXT Rich Text Format v1.0 pasteboard type");
        assert_eq!(modern, legacy, "the registry must resolve both to Rtf");
        assert_eq!(
            flavor_key(modern, "public.rtf"),
            flavor_key(legacy, "NeXT Rich Text Format v1.0 pasteboard type"),
            "two spellings of one flavor must dedupe against each other"
        );
    }

    /// Two flavors nothing recognises must NOT dedupe against each other —
    /// they are both `Flavor::Other`, and keying on the variant alone would
    /// drop the second private format a source offered.
    ///
    /// NEGATIVE CONTROL: return `format!("{known:?}")` for `Other` too.
    #[test]
    fn unrecognised_flavors_keep_their_own_identity() {
        let a = "com.example.private-one";
        let b = "com.example.private-two";
        assert_ne!(
            flavor_key(Flavor::from_uti(a), a),
            flavor_key(Flavor::from_uti(b), b)
        );
    }

    /// A multi-file selection is N pasteboard ITEMS, and the grouping in
    /// `write_payload` is what preserves that. One item offering
    /// `public.file-url` three times would be one file advertised three times.
    #[test]
    fn a_file_list_encodes_to_one_item_per_file() {
        let files = rich_clipboard::FileList::of_paths(["/tmp/a.txt", "/tmp/b.txt", "/tmp/c.txt"]);
        let payload = rich_clipboard::encode(
            &rich_clipboard::RichItem::Files(files),
            Platform::MacOs,
        )
        .expect("a file list is publishable on macOS");

        assert_eq!(
            payload.item_count(),
            3,
            "three files must become three pasteboard items"
        );
        for index in 0..3 {
            assert!(
                payload
                    .group(index)
                    .any(|i| Flavor::from_uti(&i.native) == Flavor::FileList),
                "item {index} must carry a file URL"
            );
        }
    }
}
