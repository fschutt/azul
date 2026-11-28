//! iOS file picker via `UIDocumentPickerViewController`.
//!
//! Flow:
//! 1. `dispatch_open_file` parks the caller's `FilePickerHandle` clone in the App's own registry
//!    (`AppServices::pickers`) under a fresh request ID.
//! 2. We build a `[UTType]` filter array, alloc the picker `[[UIDocumentPickerViewController alloc]
//!    initForOpeningContentTypes:asCopy:YES]` (iOS 14+; `asCopy:YES` copies the file into the app's
//!    tmp sandbox so the caller gets a regular `file://` URL with no security-scoped bracketing
//!    required).
//! 3. We alloc an `AzulDocumentPickerDelegate` NSObject (registered once via
//!    `objc::declare::ClassDecl`, same pattern as `AzulGestureTarget`), set its `requestID` ivar,
//!    and attach it to the picker via `objc_setAssociatedObject` (so the picker retains the
//!    delegate for its lifetime — UIKit doesn't retain delegates itself).
//! 4. Present from the key window's root view controller.
//! 5. Two delegate selectors read `requestID` back out, take the handle out of the registry,
//!    write the status, and remove the entry.
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
use std::sync::Once;

use azul_css::{corety::OptionString, AzString, OptionStringVec, StringVec};
#[cfg(target_os = "ios")]
use objc::declare::ClassDecl;
#[cfg(target_os = "ios")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "ios")]
use objc::{class, msg_send, sel, sel_impl};

use super::{FilePickerHandle, FilePickerStatus};

/// What a picker on the screen was asked for.
#[cfg(target_os = "ios")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickKind {
    /// A file or a folder: the URLs are what the delegate answers.
    Open,
    /// A place to save: the URLs are where the person saved the file, and the file of the bytes in the scratch folder goes.
    Export,
    /// A folder: its access is kept open for the process, and a bookmark of it is kept for the next start.
    Folder,
}

/// What a picker on the screen waits for: the handle the app polls. Parked in the App's own registry (`AppServices::pickers`) under
/// the id the delegate carries (its `requestID` ivar): no process-wide map and counter.
#[cfg(target_os = "ios")]
struct PendingPick {
    handle: FilePickerHandle,
    kind: PickKind,
    /// The folder of the file an export hands to the picker: removed when the picker is done.
    scratch: Option<std::path::PathBuf>,
}

/// Park the request; its id, or `None` when the app has no services yet or too many pickers wait.
#[cfg(target_os = "ios")]
fn register_handle(handle: FilePickerHandle) -> Option<u64> {
    register_pick(handle, PickKind::Open, None)
}

#[cfg(target_os = "ios")]
fn register_pick(handle: FilePickerHandle, kind: PickKind, scratch: Option<std::path::PathBuf>) -> Option<u64> {
    crate::desktop::process_app::services()?
        .pickers
        .register(PendingPick { handle, kind, scratch })
        .ok()
}

/// Take the request `request_id` back out (an answer that comes twice finds nothing).
#[cfg(target_os = "ios")]
fn pop_pending(request_id: u64) -> Option<PendingPick> {
    crate::desktop::process_app::services()?
        .pickers
        .take::<PendingPick>(request_id)
}

/// [`pop_pending`] for a caller that wants only the handle (the scratch folder, if any, is removed).
#[cfg(target_os = "ios")]
fn pop_handle(request_id: u64) -> Option<FilePickerHandle> {
    let pending = pop_pending(request_id)?;
    clean_scratch(pending.scratch.as_deref());
    Some(pending.handle)
}

/// Remove the folder an export wrote its file to (a failure is ignored: it is the system's temporary folder).
#[cfg(target_os = "ios")]
fn clean_scratch(scratch: Option<&std::path::Path>) {
    if let Some(dir) = scratch {
        drop(std::fs::remove_dir_all(dir));
    }
}

/// Why a request that cannot be parked is answered with an error and shows no picker.
#[cfg(target_os = "ios")]
const NOT_TRACKABLE: &str = "too many pickers are waiting for an answer (or the app has not started its services): finish or cancel one first";

/// `+[UTType typeWithIdentifier:]` for a uniform type identifier; null when the framework has no
/// such type (or is not loaded).
///
/// `UTType`'s well-known types are Objective-C global constants (`UTTypePNG`, `UTTypeData`,
/// `UTTypeFolder`), NOT class methods: `+[UTType png]`, `+[UTType data]` and `+[UTType folder]`
/// do not exist, and this file sent them (`performSelector:` with a selector registered from
/// `"png\0"`, which panics before it is sent) - so every use of the picker aborted.
#[cfg(target_os = "ios")]
unsafe fn ut_type_with_identifier(identifier: &str) -> *mut Object {
    let Some(cls) = Class::get("UTType") else {
        return ptr::null_mut();
    };
    let ns = nsstring_from_str(identifier);
    msg_send![cls, typeWithIdentifier: ns]
}

/// Convert a filter descriptor (e.g. "*.png", "png", "image/png") into a `UTType *`
/// ([`uti_query_for_descriptor`] decides how to ask). Returns null for descriptors that map to
/// nothing.
#[cfg(target_os = "ios")]
unsafe fn ut_type_for_descriptor(descriptor: &str) -> *mut Object {
    use crate::desktop::shell2::common::ios_logic::{uti_query_for_descriptor, UtiQuery};

    let Some(cls) = Class::get("UTType") else {
        return ptr::null_mut();
    };
    match uti_query_for_descriptor(descriptor) {
        None => ptr::null_mut(),
        Some(UtiQuery::Identifier(identifier)) => ut_type_with_identifier(identifier),
        Some(UtiQuery::Extension(extension)) => {
            let ns = nsstring_from_str(&extension);
            msg_send![cls, typeWithFilenameExtension: ns]
        }
        Some(UtiQuery::MimeType(mime)) => {
            let ns = nsstring_from_str(&mime);
            msg_send![cls, typeWithMIMEType: ns]
        }
    }
}

#[cfg(target_os = "ios")]
unsafe fn nsstring_from_str(s: &str) -> *mut Object {
    let mut bytes = Vec::with_capacity(s.len() + 1);
    bytes.extend_from_slice(s.as_bytes());
    bytes.push(0);
    let cstr = bytes.as_ptr() as *const i8;
    let ns: *mut Object = msg_send![class!(NSString), stringWithUTF8String: cstr];
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
        let data_t = ut_type_with_identifier(crate::desktop::shell2::common::ios_logic::UTI_DATA);
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
        let Some(pending) = pop_pending(request_id) else {
            return;
        };
        let handle = pending.handle.clone();
        // the file an export handed to the picker has been copied to where the person chose: it goes either way
        let export = pending.kind == PickKind::Export;
        let folder = pending.kind == PickKind::Folder;
        clean_scratch(pending.scratch.as_deref());

        if urls.is_null() {
            handle.set_status(FilePickerStatus::Cancelled);
            return;
        }
        let count: usize = msg_send![urls, count];
        if count == 0 {
            handle.set_status(FilePickerStatus::Cancelled);
            return;
        }
        if export {
            // `asCopy:YES`: the picker copied the file to the place the person chose before it answered; the answer is the name it
            // was saved under
            let url: *mut Object = msg_send![urls, objectAtIndex: 0usize];
            let name = url_to_path_string(url)
                .and_then(|path| std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()));
            handle.set_status(match name {
                Some(name) => FilePickerStatus::Selected(AzString::from(name)),
                None => FilePickerStatus::Error(AzString::from("the system saved the file but named no place for it")),
            });
            return;
        }

        let mut paths: Vec<AzString> = Vec::with_capacity(count);
        for i in 0..count {
            let url: *mut Object = msg_send![urls, objectAtIndex: i];
            // A URL picked IN PLACE (`asCopy:NO`: the directory picker) is security-scoped: it can
            // only be read between `startAccessingSecurityScopedResource` and the matching stop.
            // The caller gets a bare path, so the access is started here and kept for the life of
            // the process (what an app that keeps a folder open wants); for a copy in the app's
            // own tmp (`asCopy:YES`) the call answers NO and changes nothing.
            if !url.is_null() {
                let _started: bool = msg_send![url, startAccessingSecurityScopedResource];
                if folder {
                    // a bookmark of the folder, so that it is in `Documents::places()` after a restart
                    crate::desktop::extra::documents::apple::remember_folder(url);
                }
            }
            if let Some(p) = url_to_path_string(url) {
                paths.push(AzString::from(p));
            }
        }

        let status = if paths.is_empty() {
            FilePickerStatus::Cancelled
        } else if count == 1 {
            FilePickerStatus::Selected(paths.remove(0))
        } else {
            FilePickerStatus::SelectedMultiple(StringVec::from(paths))
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
fn get_or_create_delegate_class() -> Option<&'static Class> {
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        // The next free name when this one is taken (see `shell2::ios::declare_class`), `None`
        // when even that fails: the picker then reports an error instead of aborting the app.
        let Some(mut decl) =
            crate::desktop::shell2::ios::declare_class("AzulDocumentPickerDelegate", class!(NSObject))
        else {
            return;
        };
        decl.add_ivar::<u64>("requestID");
        decl.add_method(
            sel!(documentPicker:didPickDocumentsAtURLs:),
            document_picker_did_pick as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(documentPickerWasCancelled:),
            document_picker_was_cancelled as extern "C" fn(&Object, Sel, *mut Object),
        );
        CLS = decl.register();
    });
    // SAFETY: a registered class is never freed; null when it could not be declared.
    unsafe { CLS.as_ref() }
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
    let Some(request_id) = register_handle(handle.clone()) else {
        handle.set_status(FilePickerStatus::Error(AzString::from(NOT_TRACKABLE)));
        return;
    };

    unsafe {
        let root = key_root_view_controller();
        if root.is_null() {
            // No presentation context (e.g. app launched but the first
            // scene hasn't attached yet). Drop the registry entry and
            // resolve the handle to an error so the caller doesn't hang.
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error(AzString::from(
                "no key window — file picker cannot present",
            )));
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
            handle.set_status(FilePickerStatus::Error(AzString::from(
                "UIDocumentPickerViewController alloc failed",
            )));
            return;
        }

        let _: () = msg_send![picker, setAllowsMultipleSelection: allow_multiple];

        // Construct delegate.
        let Some(delegate_cls) = get_or_create_delegate_class() else {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error(AzString::from(
                "the file picker's delegate class could not be declared",
            )));
            return;
        };
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

/// Export: the bytes are written to a file named `suggested_name` in a folder of the app's temporary directory, and the export picker
/// (`UIDocumentPickerViewController initForExportingURLs:asCopy:YES`) copies that file to the place the person chooses; the answer is
/// the name it was saved under, after the copy. The temporary file is removed when the picker is done, whatever the person did.
#[cfg(target_os = "ios")]
pub fn dispatch_export(handle: FilePickerHandle, suggested_name: AzString, _mime: AzString, bytes: azul_css::U8Vec) {
    use azul_layout::managers::{share::sanitize_display_name, share_area::new_token};

    let name = sanitize_display_name(suggested_name.as_str(), 128);
    let scratch = std::env::temp_dir().join(format!("azul-export-{}", new_token()));
    let file = scratch.join(&name);
    let written = std::fs::create_dir_all(&scratch).and_then(|()| std::fs::write(&file, bytes.as_ref()));
    if let Err(e) = written {
        clean_scratch(Some(&scratch));
        handle.set_status(FilePickerStatus::Error(AzString::from(format!(
            "the file could not be prepared for the export dialog: {e}"
        ))));
        return;
    }
    let Some(request_id) = register_pick(handle.clone(), PickKind::Export, Some(scratch.clone())) else {
        clean_scratch(Some(&scratch));
        handle.set_status(FilePickerStatus::Error(AzString::from(NOT_TRACKABLE)));
        return;
    };

    unsafe {
        let root = key_root_view_controller();
        if root.is_null() {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error(AzString::from("no key window — the export dialog cannot present")));
            return;
        }
        let path = nsstring_from_str(&file.to_string_lossy());
        let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: path];
        let urls: *mut Object = msg_send![class!(NSArray), arrayWithObject: url];
        let picker_alloc: *mut Object = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut Object = msg_send![
            picker_alloc,
            initForExportingURLs: urls
                          asCopy: true
        ];
        if picker.is_null() {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error(AzString::from("UIDocumentPickerViewController alloc failed")));
            return;
        }
        let Some(delegate_cls) = get_or_create_delegate_class() else {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error(AzString::from(
                "the file picker's delegate class could not be declared",
            )));
            return;
        };
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
pub fn dispatch_export(handle: FilePickerHandle, _suggested_name: AzString, _mime: AzString, _bytes: azul_css::U8Vec) {
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
    let Some(request_id) = register_pick(handle.clone(), PickKind::Folder, None) else {
        handle.set_status(FilePickerStatus::Error(AzString::from(NOT_TRACKABLE)));
        return;
    };

    unsafe {
        let root = key_root_view_controller();
        if root.is_null() {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error(AzString::from(
                "no key window — directory picker cannot present",
            )));
            return;
        }

        // Single-element `[UTType folder]` content-type array. `asCopy:NO`
        // because a directory is opened in place — the returned URL is
        // security-scoped; the delegate starts the access (the caller only gets a path).
        let folder_t = ut_type_with_identifier(crate::desktop::shell2::common::ios_logic::UTI_FOLDER);
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
            handle.set_status(FilePickerStatus::Error(AzString::from(
                "UIDocumentPickerViewController alloc failed",
            )));
            return;
        }

        let _: () = msg_send![picker, setAllowsMultipleSelection: false];

        let Some(delegate_cls) = get_or_create_delegate_class() else {
            let _ = pop_handle(request_id);
            handle.set_status(FilePickerStatus::Error(AzString::from(
                "the directory picker's delegate class could not be declared",
            )));
            return;
        };
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
