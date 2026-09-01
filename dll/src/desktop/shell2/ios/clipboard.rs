//! iOS clipboard transport: `UIPasteboard` ⇄ [`ClipboardPayload`].
//!
//! iOS shares macOS's **UTI vocabulary** — `public.utf8-plain-text`,
//! `public.rtf`, `public.html`, `public.png`, `public.file-url` — so this
//! reuses `Platform::MacOs` for flavor naming rather than introducing an iOS
//! table. The two differ in the *object model*, not the type names.
//!
//! # Four ways `UIPasteboard` is not `NSPasteboard`
//!
//! * **There are no item OBJECTS.** macOS allocates `NSPasteboardItem`s and
//!   calls `setData:forType:` on each; iOS models the whole pasteboard as
//!   `[[String: Any]]` — an array of dictionaries keyed by UTI. So a write
//!   builds `NSArray<NSDictionary<NSString, NSData>>` and hands it over in one
//!   `setItems:` call.
//!
//! * **`setItems:` REPLACES everything.** There is no `clearContents` /
//!   `declareTypes:` two-step to get wrong; the array you pass is the
//!   pasteboard afterwards.
//!
//! * **Nothing is a promise.** The macOS caveat that `dataForType:` may return
//!   nil for an advertised type does not apply — every value in `items` is
//!   already materialised, so a key that is present has data.
//!
//! * **Reading is user-visible.** Since iOS 14 a paste raises a system banner
//!   ("… pasted from …"), which is why this does NOT read the pasteboard
//!   speculatively — only in response to an actual paste. `detectPatterns` is
//!   the API for probing without the banner, and is deliberately not used
//!   here: azul pastes what the user asked for, it does not sniff.
//!
//! The convenience properties (`string`, `image`, `url`) are avoided on
//! purpose: each collapses the payload to a single flavor, which is exactly
//! the flattening the typed-payload layer exists to prevent.

use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use rich_clipboard::{ClipboardItem, ClipboardPayload, Platform};

use super::super::common::clipboard::MAX_FLAVOR_BYTES;

/// `[UIPasteboard generalPasteboard]`.
unsafe fn general() -> *mut Object {
    let cls = class!(UIPasteboard);
    msg_send![cls, generalPasteboard]
}

/// Copy an `NSString` into a Rust `String`.
unsafe fn ns_string_to_rust(s: *mut Object) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let utf8: *const core::ffi::c_char = msg_send![s, UTF8String];
    if utf8.is_null() {
        return None;
    }
    Some(
        core::ffi::CStr::from_ptr(utf8)
            .to_string_lossy()
            .into_owned(),
    )
}

/// Build an autoreleased `NSString` from a Rust `&str`.
unsafe fn ns_string(s: &str) -> *mut Object {
    let cstr = std::ffi::CString::new(s).unwrap_or_default();
    let cls = class!(NSString);
    msg_send![cls, stringWithUTF8String: cstr.as_ptr()]
}

/// Read the general pasteboard as a typed payload.
pub fn read_payload() -> Option<ClipboardPayload> {
    unsafe {
        let pb = general();
        if pb.is_null() {
            return None;
        }
        // `items` is the array of per-item dictionaries. One item with three
        // keys is one thing offered three ways; three items with one key each
        // is three things — the same distinction the macOS transport draws
        // between `pasteboardItems` and the flattened `types` union.
        let items: *mut Object = msg_send![pb, items];
        if items.is_null() {
            return None;
        }
        let item_count: usize = msg_send![items, count];
        if item_count == 0 {
            return None;
        }

        // Same constructor the macOS transport uses: iOS shares the UTI
        // vocabulary, so the payload is tagged MacOs and every flavor lookup
        // downstream resolves identically.
        let mut payload = ClipboardPayload::new(Platform::MacOs);
        for index in 0..item_count {
            let dict: *mut Object = msg_send![items, objectAtIndex: index];
            if dict.is_null() {
                continue;
            }
            let keys: *mut Object = msg_send![dict, allKeys];
            if keys.is_null() {
                continue;
            }
            let key_count: usize = msg_send![keys, count];
            for k in 0..key_count {
                let key: *mut Object = msg_send![keys, objectAtIndex: k];
                let Some(uti) = ns_string_to_rust(key) else {
                    continue;
                };
                let value: *mut Object = msg_send![dict, objectForKey: key];
                if value.is_null() {
                    continue;
                }
                // A value can be any plist type; only NSData carries bytes.
                // A string value (which UIPasteboard produces for the
                // convenience setters) is converted rather than skipped, or a
                // paste from an app that used `pasteboard.string = …` would
                // come back empty.
                let is_data: bool = msg_send![value, isKindOfClass: class!(NSData)];
                let bytes: Vec<u8> = if is_data {
                    let len: usize = msg_send![value, length];
                    if len as u64 > MAX_FLAVOR_BYTES {
                        continue;
                    }
                    let ptr: *const u8 = msg_send![value, bytes];
                    if ptr.is_null() {
                        continue;
                    }
                    core::slice::from_raw_parts(ptr, len).to_vec()
                } else {
                    let is_str: bool = msg_send![value, isKindOfClass: class!(NSString)];
                    if !is_str {
                        continue;
                    }
                    match ns_string_to_rust(value) {
                        Some(s) => s.into_bytes(),
                        None => continue,
                    }
                };
                payload.push(ClipboardItem::in_item(index, uti, bytes));
            }
        }
        if payload.is_empty() {
            None
        } else {
            Some(payload)
        }
    }
}

/// Publish a typed payload to the general pasteboard.
///
/// Every flavor of the fan-out is published, which is what makes a paste land
/// in Pages or Mail as styled text rather than flattened plain text.
pub fn write_payload(payload: &ClipboardPayload) -> bool {
    unsafe {
        let pb = general();
        if pb.is_null() {
            return false;
        }

        // Group the flat entry list by pasteboard item, exactly as the macOS
        // transport does — `encode` emits best-flavor-first and a reader is
        // entitled to take the first type it recognises, so order matters
        // within a group.
        let mut groups: Vec<Vec<&ClipboardItem>> = Vec::new();
        for entry in payload.items() {
            if groups.len() <= entry.item {
                groups.resize_with(entry.item + 1, Vec::new);
            }
            groups[entry.item].push(entry);
        }

        let array_cls = class!(NSMutableArray);
        let items: *mut Object = msg_send![array_cls, array];

        for group in &groups {
            let dict_cls = class!(NSMutableDictionary);
            let dict: *mut Object = msg_send![dict_cls, dictionary];
            let mut wrote_any = false;

            for entry in group {
                let data_cls = class!(NSData);
                let data: *mut Object = msg_send![
                    data_cls,
                    dataWithBytes: entry.bytes.as_ptr() as *const core::ffi::c_void
                    length: entry.bytes.len()
                ];
                if data.is_null() {
                    continue;
                }
                let key = ns_string(&entry.native);
                let _: () = msg_send![dict, setObject: data forKey: key];
                wrote_any = true;
            }

            // An item with no representable flavor is dropped rather than
            // published empty: an empty dictionary advertises nothing, and a
            // reader would see a phantom entry.
            if wrote_any {
                let _: () = msg_send![items, addObject: dict];
            }
        }

        let count: usize = msg_send![items, count];
        if count == 0 {
            return false;
        }
        let _: () = msg_send![pb, setItems: items];
        true
    }
}

/// Plain-text convenience read, for callers that only want a string.
pub fn get_clipboard_content() -> Option<String> {
    unsafe {
        let pb = general();
        if pb.is_null() {
            return None;
        }
        let s: *mut Object = msg_send![pb, string];
        ns_string_to_rust(s)
    }
}

/// Plain-text convenience write.
pub fn write_to_clipboard(text: &str) -> bool {
    unsafe {
        let pb = general();
        if pb.is_null() {
            return false;
        }
        let s = ns_string(text);
        let _: () = msg_send![pb, setString: s];
        true
    }
}
