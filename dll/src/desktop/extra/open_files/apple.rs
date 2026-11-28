//! iOS: `application:openURL:options:`, the entry of everything another app or the system opens this one with: a file from the
//! Files app, Mail, Safari or the share sheet ("Open in ..."), and a link of a scheme the app declared (`CFBundleURLTypes`).
//!
//! The delegate method (declared in `shell2/ios/mod.rs` on the app's delegate class, one line) calls [`did_open_url`] on the main
//! thread. A link is queued as it is. A file is copied by a worker thread into the app's own inbox
//! (`<Library>/Application Support/azul-inbox/<token>/<name>`, [`azul_layout::open_inbox`]) - with the security scope started on
//! the worker for a file opened IN PLACE (`UIApplicationOpenURLOptionsOpenInPlaceKey`: the original, which only the scope makes
//! readable, and the origin the app may write back to) and stopped when the copy is done. A file the system already placed in the
//! app's own container (`Documents/Inbox/`, a copy made by the system when the app does not open in place) is used where it is.
//! The request is queued into the app's open queue; the display tick's pump runs the app's handler.

use std::{
    ffi::{c_char, CStr, CString},
    fs::File,
    path::PathBuf,
};

use azul_core::open_files::{OpenAction, OpenRequest, OpenedItem, OpenedItemVec};
use azul_css::{AzString, OptionU64};
use azul_layout::{
    mime,
    open_inbox::{Inbox, OpenLimits},
};
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::NSString;

/// The folder of the inbox under the app's `Library/Application Support`.
const INBOX_FOLDER: &str = "azul-inbox";

/// `UIApplicationOpenURLOptionsOpenInPlaceKey`.
const OPEN_IN_PLACE_KEY: &str = "UIApplicationOpenURLOptionsOpenInPlaceKey";

/// An `NSURL` that moves to the worker thread. `NSURL` is immutable and its methods may be called from any thread.
struct SendUrl(Retained<AnyObject>);
// SAFETY: see the type's docs.
unsafe impl Send for SendUrl {}

fn class(name: &str) -> Option<&'static AnyClass> {
    let name = CString::new(name).ok()?;
    AnyClass::get(&name)
}

/// An `NSString *` as a Rust string; `None` for nil.
unsafe fn ns_string(ns: *mut AnyObject) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let utf8: *const c_char = msg_send![ns, UTF8String];
    if utf8.is_null() {
        return None;
    }
    Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
}

/// The app became active: a request that comes later is not the one that started it. Called from
/// `applicationDidBecomeActive:` (main thread).
pub fn note_active() {
    if let Some(services) = crate::desktop::process_app::services() {
        services.open.note_active();
    }
}

/// `-[AppDelegate application:openURL:options:]`: answer whether the app takes the URL. Main thread.
///
/// # Safety
///
/// `url` and `options` are the objects UIKit passes, live for the call.
pub unsafe fn did_open_url(url: *mut AnyObject, options: *mut AnyObject) -> bool {
    if url.is_null() {
        return false;
    }
    let launched_app = !crate::desktop::process_app::services().is_some_and(|s| s.open.was_active());
    let is_file: Bool = msg_send![url, isFileURL];
    if !is_file.as_bool() {
        let text: *mut AnyObject = msg_send![url, absoluteString];
        let Some(link) = ns_string(text) else {
            return false;
        };
        crate::plog_info!("[open] a link: {link}");
        queue(OpenRequest {
            action: OpenAction::View,
            items: OpenedItemVec::from_vec(vec![OpenedItem::url(AzString::from(link))]),
            launched_app,
            skipped: 0,
        });
        return true;
    }
    let in_place = open_in_place(options);
    let Some(url) = Retained::retain(url) else {
        return false;
    };
    let job = SendUrl(url);
    let spawned = std::thread::Builder::new()
        .name("azul-open".to_string())
        .spawn(move || {
            let job = job;
            let request = file_request(&job.0, in_place, launched_app);
            queue(request);
        });
    if let Err(e) = spawned {
        crate::plog_warn!("[open] cannot start the worker that copies the file: {e}");
        return false;
    }
    true
}

/// Whether the system opened the file IN PLACE (`options[UIApplicationOpenURLOptionsOpenInPlaceKey]` is YES).
unsafe fn open_in_place(options: *mut AnyObject) -> bool {
    if options.is_null() {
        return false;
    }
    let key = NSString::from_str(OPEN_IN_PLACE_KEY);
    let value: *mut AnyObject = msg_send![options, objectForKey: &*key];
    if value.is_null() {
        return false;
    }
    let yes: Bool = msg_send![value, boolValue];
    yes.as_bool()
}

/// Park the request in the app's queue (the display tick's pump runs the handler).
fn queue(request: OpenRequest) {
    match crate::desktop::process_app::services() {
        Some(services) => {
            let items = request.items.as_ref().len();
            if services.open.push(request) {
                crate::plog_info!("[open] {items} item(s) queued for the app's open handler");
            }
        }
        None => crate::plog_warn!("[open] no App is registered: the request is dropped"),
    }
}

/// The worker: one file URL as a request with one item (a failure is the item's reason).
fn file_request(url: &Retained<AnyObject>, in_place: bool, launched_app: bool) -> OpenRequest {
    let (name, path_text, origin) = unsafe {
        let last: *mut AnyObject = msg_send![&**url, lastPathComponent];
        let path: *mut AnyObject = msg_send![&**url, path];
        let absolute: *mut AnyObject = msg_send![&**url, absoluteString];
        (ns_string(last).unwrap_or_default(), ns_string(path), ns_string(absolute).unwrap_or_default())
    };
    let kind = mime::for_file_name(&name).to_string();
    let item = match path_text {
        None => OpenedItem::file_failed(
            AzString::from(name.as_str()),
            AzString::from(kind.as_str()),
            OptionU64::None,
            AzString::from(origin.as_str()),
            AzString::from("the system gave a file URL without a path"),
        ),
        Some(path) => copy_file(url, &name, &kind, &path, &origin, in_place),
    };
    OpenRequest {
        action: OpenAction::View,
        items: OpenedItemVec::from_vec(vec![item]),
        launched_app,
        skipped: 0,
    }
}

/// Where the app's own files live: `$HOME` is the app's container on iOS.
fn container() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn copy_file(url: &Retained<AnyObject>, name: &str, kind: &str, path: &str, origin: &str, in_place: bool) -> OpenedItem {
    let fail = |why: String, size: Option<u64>| {
        OpenedItem::file_failed(
            AzString::from(name),
            AzString::from(kind),
            size.map_or(OptionU64::None, OptionU64::Some),
            AzString::from(origin),
            AzString::from(why),
        )
    };
    let Some(home) = container() else {
        return fail("the app's own folder is not known".to_string(), None);
    };
    let source = PathBuf::from(path);
    // a copy the system made inside the app's container (Documents/Inbox) is the working copy already
    if !in_place && source.starts_with(&home) {
        let size = std::fs::metadata(&source).ok().map(|m| m.len());
        return OpenedItem::file(
            AzString::from(name),
            AzString::from(kind),
            size.map_or(OptionU64::None, OptionU64::Some),
            AzString::from(path),
            AzString::from(""),
            false,
        );
    }
    // the scope of an original opened in place is held for the length of the copy
    let scoped: Bool = unsafe { msg_send![&**url, startAccessingSecurityScopedResource] };
    let result = (|| {
        let mut file = File::open(&source).map_err(|e| format!("{name} cannot be read: {e}"))?;
        let size = file.metadata().ok().map(|m| m.len());
        let inbox = Inbox::new(home.join("Library").join("Application Support").join(INBOX_FOLDER));
        let mut request = inbox.begin(OpenLimits::default())?;
        request.copy_in(name, &mut file, size)
    })();
    if scoped.as_bool() {
        let _: () = unsafe { msg_send![&**url, stopAccessingSecurityScopedResource] };
    }
    match result {
        Ok(copied) => OpenedItem::file(
            AzString::from(copied.name),
            AzString::from(kind),
            OptionU64::Some(copied.size),
            AzString::from(copied.path.to_string_lossy().into_owned()),
            AzString::from(origin),
            in_place,
        ),
        Err(why) => fail(why, None),
    }
}
