//! iOS file picker via `UIDocumentPickerViewController`.
//!
//! Flow:
//! 1. `dispatch_open_file` stores the caller's `FilePickerHandle` clone in
//!    `PENDING_PICKERS` keyed by a fresh request ID.
//! 2. We build a `[UTType]` filter array, alloc the picker
//!    `[[UIDocumentPickerViewController alloc] initForOpeningContentTypes:asCopy:YES]`
//!    (iOS 14+; `asCopy:YES` copies the file into the app's tmp sandbox
//!    so the caller gets a regular `file://` URL with no security-scoped
//!    bracketing required).
//! 3. We alloc an `AzulDocumentPickerDelegate` NSObject (registered once
//!    via `objc::declare::ClassDecl`, same pattern as `AzulGestureTarget`),
//!    set its `requestID` ivar, and attach it to the picker via
//!    `objc_setAssociatedObject` (so the picker retains the delegate
//!    for its lifetime — UIKit doesn't retain delegates itself).
//! 4. Present from the key window's root view controller.
//! 5. Two delegate selectors read `requestID` back out, look up the
//!    handle in `PENDING_PICKERS`, write the status, and remove the
//!    entry.
//!
//! `Info.plist` keys are *not* needed for the picker itself — it runs
//! out-of-process and grants the app per-URL read access via the OS.
//! Optional keys for surfacing files this app owns in Files.app:
//! - `LSSupportsOpeningDocumentsInPlace = YES`
//! - `UIFileSharingEnabled = YES`
//!
//! Save/directory dispatchers remain stubs in this tick — the open-file
//! path is what AzulPaint (P2) and AzulDoc (P5) need.

#![allow(non_snake_case)]

#[cfg(target_os = "ios")]
use std::os::raw::c_void;
#[cfg(target_os = "ios")]
use std::ptr;
#[cfg(target_os = "ios")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "ios")]
use std::sync::Mutex;

#[cfg(target_os = "ios")]
use objc::declare::ClassDecl;
#[cfg(target_os = "ios")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "ios")]
use objc::{class, msg_send, sel, sel_impl};

#[cfg(target_os = "ios")]
use std::collections::BTreeMap;
#[cfg(target_os = "ios")]
use std::sync::Once;

use azul_css::{corety::OptionString, AzString, OptionStringVec, StringVec};

use super::{FilePickerHandle, FilePickerStatus};

#[cfg(target_os = "ios")]
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "ios")]
static PENDING_PICKERS: Mutex<BTreeMap<u64, FilePickerHandle>> = Mutex::new(BTreeMap::new());

#[cfg(target_os = "ios")]
fn allocate_request_id() -> u64 {
    REQUEST_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(target_os = "ios")]
fn register_handle(handle: FilePickerHandle) -> u64 {
    let id = allocate_request_id();
    if let Ok(mut g) = PENDING_PICKERS.lock() {
        g.insert(id, handle);
    }
    id
}

#[cfg(target_os = "ios")]
fn pop_handle(request_id: u64) -> Option<FilePickerHandle> {
    PENDING_PICKERS.lock().ok().and_then(|mut g| g.remove(&request_id))
}

/// Convert a UTI-style extension descriptor (e.g. "*.png", "png", "image/png")
/// into a `UTType *`. iOS 14+ class methods are preferred for known types;
/// arbitrary extensions fall back to `[UTType typeWithFilenameExtension:]`.
/// Returns null for descriptors that map to nothing.
#[cfg(target_os = "ios")]
unsafe fn ut_type_for_descriptor(descriptor: &str) -> *mut Object {
    let cls = class!(UTType);
    // Strip leading "*." / "."
    let trimmed = descriptor
        .trim_start_matches('*')
        .trim_start_matches('.')
        .to_ascii_lowercase();

    // Common types via UTType class methods.
    let class_method = match trimmed.as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpeg"),
        "pdf" => Some("pdf"),
        "gif" => Some("gif"),
        "txt" | "text" => Some("plainText"),
        "json" => Some("json"),
        "html" | "htm" => Some("html"),
        "xml" => Some("xml"),
        "zip" => Some("zip"),
        "mov" => Some("movie"),
        "mp4" => Some("mpeg4Movie"),
        "mp3" => Some("mp3"),
        "wav" => Some("wav"),
        "svg" => Some("svg"),
        _ => None,
    };

    if let Some(method) = class_method {
        let sel = objc::runtime::Sel::register(
            &format!("{}\0", method),
        );
        let t: *mut Object = msg_send![cls, performSelector: sel];
        if !t.is_null() {
            return t;
        }
    }

    if trimmed.is_empty() {
        return ptr::null_mut();
    }

    let ns_ext = nsstring_from_str(&trimmed);
    let t: *mut Object = msg_send![cls, typeWithFilenameExtension: ns_ext];
    t
}

#[cfg(target_os = "ios")]
unsafe fn nsstring_from_str(s: &str) -> *mut Object {
    let mut bytes = Vec::with_capacity(s.len() + 1);
    bytes.extend_from_slice(s.as_bytes());
    bytes.push(0);
    let cstr = bytes.as_ptr() as *const i8;
    let ns: *mut Object =
        msg_send![class!(NSString), stringWithUTF8String: cstr];
    ns
}

#[cfg(target_os = "ios")]
unsafe fn nsstring_to_string(ns: *mut Object) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let cstr: *const i8 = msg_send![ns, UTF8String];
    if cstr.is_null() {
        return None;
    }
    let bytes = core::ffi::CStr::from_ptr(cstr);
    bytes.to_str().ok().map(|s| s.to_owned())
}

#[cfg(target_os = "ios")]
unsafe fn url_to_path_string(url: *mut Object) -> Option<String> {
    if url.is_null() {
        return None;
    }
    let path: *mut Object = msg_send![url, path];
    nsstring_to_string(path)
}

/// Build an `NSArray<UTType *>` from a `StringVec` of filter descriptors.
/// Returns `(NSArray *, used_default)`. If `filter_descriptors` is empty
/// or every descriptor failed to resolve, falls back to `UTType.data`.
#[cfg(target_os = "ios")]
unsafe fn build_uttype_array(filter_descriptors: &OptionStringVec) -> *mut Object {
    let mut resolved: Vec<*mut Object> = Vec::new();
    if let Some(list) = filter_descriptors.as_option() {
        for s in list.as_ref().iter() {
            let t = ut_type_for_descriptor(s.as_str());
            if !t.is_null() {
                resolved.push(t);
            }
        }
    }

    if resolved.is_empty() {
        let data_t: *mut Object = msg_send![class!(UTType), data];
        if !data_t.is_null() {
            resolved.push(data_t);
        }
    }

    let arr_cls = class!(NSArray);
    let arr: *mut Object = if resolved.is_empty() {
        msg_send![arr_cls, array]
    } else {
        msg_send![
            arr_cls,
            arrayWithObjects: resolved.as_ptr()
                       count: resolved.len() as usize
        ]
    };
    arr
}

// ───── Delegate class ──────────────────────────────────────────────────

#[cfg(target_os = "ios")]
extern "C" fn document_picker_did_pick(
    this: &Object,
    _cmd: Sel,
    _picker: *mut Object,
    urls: *mut Object,
) {
    unsafe {
        let request_id: u64 = *this.get_ivar("requestID");
        let handle = pop_handle(request_id);
        if handle.is_none() {
            return;
        }
        let handle = handle.unwrap();

        if urls.is_null() {
            handle.set_status(FilePickerStatus::Cancelled);
            return;
        }
        let count: usize = msg_send![urls, count];
        if count == 0 {
            handle.set_status(FilePickerStatus::Cancelled);
            return;
        }

        let mut paths: Vec<AzString> = Vec::with_capacity(count);
        for i in 0..count {
            let url: *mut Object = msg_send![urls, objectAtIndex: i];
            if let Some(p) = url_to_path_string(url) {
                paths.push(AzString::from(p));
            }
        }

        let status = if paths.is_empty() {
            FilePickerStatus::Cancelled
        } else if count == 1 {
            FilePickerStatus::Selected {
                path: OptionString::Some(paths.remove(0)),
            }
        } else {
            FilePickerStatus::SelectedMultiple {
                paths: StringVec::from(paths),
            }
        };
        handle.set_status(status);
    }
}

#[cfg(target_os = "ios")]
extern "C" fn document_picker_was_cancelled(this: &Object, _cmd: Sel, _picker: *mut Object) {
    unsafe {
        let request_id: u64 = *this.get_ivar("requestID");
        if let Some(handle) = pop_handle(request_id) {
            handle.set_status(FilePickerStatus::Cancelled);
        }
    }
}

#[cfg(target_os = "ios")]
fn get_or_create_delegate_class() -> &'static Class {
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("AzulDocumentPickerDelegate", superclass).unwrap();
        decl.add_ivar::<u64>("requestID");
        decl.add_method(
            sel!(documentPicker:didPickDocumentsAtURLs:),
            document_picker_did_pick
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(documentPickerWasCancelled:),
            document_picker_was_cancelled as extern "C" fn(&Object, Sel, *mut Object),
        );
        CLS = decl.register();
    });
    unsafe { &*CLS }
}

#[cfg(target_os = "ios")]
unsafe fn key_root_view_controller() -> *mut Object {
    let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
    if app.is_null() {
        return ptr::null_mut();
    }
    // iOS 13+ multi-scene apps don't populate `keyWindow` on UIApplication;
    // walk `connectedScenes` to find an active foreground window. Fall back
    // to the deprecated `keyWindow` on older iOS.
    let mut window: *mut Object = msg_send![app, keyWindow];
    if window.is_null() {
        let scenes: *mut Object = msg_send![app, connectedScenes];
        if !scenes.is_null() {
            let enumerator: *mut Object = msg_send![scenes, objectEnumerator];
            loop {
                let scene: *mut Object = msg_send![enumerator, nextObject];
                if scene.is_null() {
                    break;
                }
                let windows: *mut Object = msg_send![scene, windows];
                if windows.is_null() {
                    continue;
                }
                let wcount: usize = msg_send![windows, count];
                if wcount == 0 {
                    continue;
                }
                window = msg_send![windows, objectAtIndex: 0usize];
                if !window.is_null() {
                    break;
                }
            }
        }
    }
    if window.is_null() {
        return ptr::null_mut();
    }
    let root: *mut Object = msg_send![window, rootViewController];
    root
}

/// objc_setAssociatedObject — attach `delegate` to `picker` so the picker
/// retains it for its lifetime (UIKit delegate properties are weak).
/// `OBJC_ASSOCIATION_RETAIN_NONATOMIC = 1`.
#[cfg(target_os = "ios")]
unsafe fn associate_strong(picker: *mut Object, delegate: *mut Object) {
    extern "C" {
        fn objc_setAssociatedObject(
            object: *mut Object,
            key: *const c_void,
            value: *mut Object,
            policy: usize,
        );
    }
    // Static address used as the associated-object key.
    static KEY: u8 = 0;
    objc_setAssociatedObject(picker, &KEY as *const u8 as *const c_void, delegate, 1);
}

// ───── Public entry points ────────────────────────────────────────────

#[cfg(target_os = "ios")]
pub fn dispatch_open_file(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
    filter_descriptors: OptionStringVec,
    allow_multiple: bool,
) {
    let request_id = register_handle(handle.clone());

    unsafe {
        let root = key_root_view_controller();
        if root.is_null() {
            // No presentation context (e.g. app launched but the first
            // scene hasn't attached yet). Drop the registry entry and
            // resolve the handle to an error so the caller doesn't hang.
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error {
                message: AzString::from("no key window — file picker cannot present"),
            });
            return;
        }

        let types_array = build_uttype_array(&filter_descriptors);

        // initForOpeningContentTypes:asCopy:
        let picker_cls = class!(UIDocumentPickerViewController);
        let picker_alloc: *mut Object = msg_send![picker_cls, alloc];
        let picker: *mut Object = msg_send![
            picker_alloc,
            initForOpeningContentTypes: types_array
                                asCopy: true
        ];
        if picker.is_null() {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error {
                message: AzString::from("UIDocumentPickerViewController alloc failed"),
            });
            return;
        }

        let _: () = msg_send![picker, setAllowsMultipleSelection: allow_multiple];

        // Construct delegate.
        let delegate_cls = get_or_create_delegate_class();
        let delegate_alloc: *mut Object = msg_send![delegate_cls, alloc];
        let delegate: *mut Object = msg_send![delegate_alloc, init];
        (*delegate).set_ivar::<u64>("requestID", request_id);

        let _: () = msg_send![picker, setDelegate: delegate];
        associate_strong(picker, delegate);

        let _: () = msg_send![
            root,
            presentViewController: picker
                          animated: true
                        completion: ptr::null_mut::<Object>()
        ];
    }
}

#[cfg(not(target_os = "ios"))]
pub fn dispatch_open_file(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
    _filter_descriptors: OptionStringVec,
    _allow_multiple: bool,
) {
    handle.set_status(FilePickerStatus::Cancelled);
}

pub fn dispatch_save_file(handle: FilePickerHandle, _title: AzString, _default_path: OptionString) {
    // Deferred — needs an API decision, not just mechanical wiring.
    // iOS has no "choose a destination then hand me a path to write"
    // dialog: `UIDocumentPickerViewController initForExportingURLs:` only
    // *exports files that already exist*. This signature carries a
    // suggested title but no source file/bytes, so a faithful iOS save
    // needs either (a) the dialog API to carry the source URL/bytes, or
    // (b) the caller to write into a directory chosen via the now-real
    // `dispatch_open_directory` below. Until that's settled, resolve to
    // Cancelled rather than ship an export of an empty placeholder.
    handle.set_status(FilePickerStatus::Cancelled);
}

/// Directory picker via `UIDocumentPickerViewController
/// initForOpeningContentTypes:[UTTypeFolder] asCopy:NO`. Reuses the same
/// delegate + `nativeOnResult` readback as the open-file path; the
/// delegate reports the chosen folder's `url.path`.
#[cfg(target_os = "ios")]
pub fn dispatch_open_directory(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
) {
    let request_id = register_handle(handle.clone());

    unsafe {
        let root = key_root_view_controller();
        if root.is_null() {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error {
                message: AzString::from("no key window — directory picker cannot present"),
            });
            return;
        }

        // Single-element `[UTType folder]` content-type array. `asCopy:NO`
        // because a directory is opened in place — the returned URL is
        // security-scoped, so callers that read its contents must bracket
        // access with start/stopAccessingSecurityScopedResource.
        let folder_t: *mut Object = msg_send![class!(UTType), folder];
        let arr_cls = class!(NSArray);
        let types_array: *mut Object = if folder_t.is_null() {
            msg_send![arr_cls, array]
        } else {
            let objs = [folder_t];
            msg_send![
                arr_cls,
                arrayWithObjects: objs.as_ptr()
                           count: 1usize
            ]
        };

        let picker_cls = class!(UIDocumentPickerViewController);
        let picker_alloc: *mut Object = msg_send![picker_cls, alloc];
        let picker: *mut Object = msg_send![
            picker_alloc,
            initForOpeningContentTypes: types_array
                                asCopy: false
        ];
        if picker.is_null() {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error {
                message: AzString::from("UIDocumentPickerViewController alloc failed"),
            });
            return;
        }

        let _: () = msg_send![picker, setAllowsMultipleSelection: false];

        let delegate_cls = get_or_create_delegate_class();
        let delegate_alloc: *mut Object = msg_send![delegate_cls, alloc];
        let delegate: *mut Object = msg_send![delegate_alloc, init];
        (*delegate).set_ivar::<u64>("requestID", request_id);

        let _: () = msg_send![picker, setDelegate: delegate];
        associate_strong(picker, delegate);

        let _: () = msg_send![
            root,
            presentViewController: picker
                          animated: true
                        completion: ptr::null_mut::<Object>()
        ];
    }
}

#[cfg(not(target_os = "ios"))]
pub fn dispatch_open_directory(
    handle: FilePickerHandle,
    _title: AzString,
    _default_path: OptionString,
) {
    handle.set_status(FilePickerStatus::Cancelled);
}
