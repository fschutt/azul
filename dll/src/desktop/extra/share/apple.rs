//! iOS: the system's activity view (and the open-in menu) for `CallbackInfo::share`, written with `objc2` like the other
//! runtime-looked-up UIKit backends (`auth_session/apple.rs`): classes by name at run time, plain `msg_send!`.
//!
//! * The files of the plan are COPIED into `<tmp>/azul-share/<token>/` ([`ShareArea::stage`]) under the names the app gave
//!   them and handed over as file URLs, so the receiver (an extension in another process) is given exactly those files, with
//!   those names, never the app's own folder. A text goes as an `NSString`, a link as an `NSURL`.
//! * `UIActivityViewController` is presented over the topmost view controller of the key window. On an iPad it MUST be
//!   anchored (its popover presentation controller has no source and the app is terminated with an exception when it is
//!   presented without one): the request's anchor rect, else the middle of the window, without an arrow.
//! * "Open with" is `UIDocumentInteractionController`'s open-in menu for the one file; the controller must outlive the menu,
//!   so it is attached to the presenting view controller (an associated object, released with it or by the next open-in).
//! * UIKit is main-thread only: the callbacks of this shell run on the main thread; a call from another is refused with
//!   the reason, not dispatched (the answer would no longer be the real one).

use std::{ffi::CString, os::raw::c_void, path::PathBuf};

use azul_core::{geom::LogicalRect, share::ShareStatus};
use azul_layout::managers::{
    share::{PlannedItem, SharePlan},
    share_area::ShareArea,
};
use objc2::{
    encode::{Encode, Encoding},
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::NSString;

/// The folder of the staged shares under the app's temporary directory.
const AREA_FOLDER: &str = "azul-share";

/// `CGPoint` (64-bit: `CGFloat` is a double).
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

/// `CGSize`.
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

/// `CGRect`.
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

// SAFETY: the layouts above are those of the Core Graphics structs on every 64-bit Apple target.
unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

fn class(name: &str) -> Option<&'static AnyClass> {
    let name = CString::new(name).ok()?;
    AnyClass::get(&name)
}

fn on_main_thread() -> bool {
    class("NSThread").is_some_and(|thread| {
        let main: Bool = unsafe { msg_send![thread, isMainThread] };
        main.as_bool()
    })
}

/// Show the activity view (or the open-in menu) for `plan`, or say why not.
pub(super) fn share(plan: &SharePlan) -> ShareStatus {
    if !on_main_thread() {
        return ShareStatus::Failed(
            "the share sheet can only be shown from the main thread (call share from a callback)"
                .to_string()
                .into(),
        );
    }
    let mut staged: Vec<PathBuf> = Vec::new();
    let mut staged_dir: Option<PathBuf> = None;
    if plan.files().next().is_some() {
        let area = ShareArea::new(PathBuf::from(azul_layout::file::temp_dir().as_str()).join(AREA_FOLDER));
        match area.stage(plan, &plan.limits) {
            Ok(done) => {
                staged = done.files.iter().map(|f| f.path.clone()).collect();
                staged_dir = Some(done.dir);
            }
            Err(status) => return status,
        }
    }
    let status = unsafe { present(plan, &staged) };
    if !matches!(status, ShareStatus::Shown) {
        if let Some(dir) = staged_dir {
            drop(std::fs::remove_dir_all(dir));
        }
    }
    status
}

/// `[UIApplication sharedApplication].keyWindow`, else the first window of the first scene.
unsafe fn key_window() -> *mut AnyObject {
    let Some(app_class) = class("UIApplication") else {
        return core::ptr::null_mut();
    };
    let app: *mut AnyObject = msg_send![app_class, sharedApplication];
    if app.is_null() {
        return core::ptr::null_mut();
    }
    let window: *mut AnyObject = msg_send![app, keyWindow];
    if !window.is_null() {
        return window;
    }
    let windows: *mut AnyObject = msg_send![app, windows];
    if windows.is_null() {
        return core::ptr::null_mut();
    }
    msg_send![windows, firstObject]
}

/// The view controller a sheet is presented from: the root of the key window, then the topmost of what it presents.
unsafe fn presenter() -> *mut AnyObject {
    let window = key_window();
    if window.is_null() {
        return core::ptr::null_mut();
    }
    let mut top: *mut AnyObject = msg_send![window, rootViewController];
    while !top.is_null() {
        let next: *mut AnyObject = msg_send![top, presentedViewController];
        if next.is_null() {
            break;
        }
        top = next;
    }
    top
}

fn rect_of(anchor: Option<LogicalRect>, bounds: CGRect) -> CGRect {
    match anchor {
        Some(rect) => CGRect {
            origin: CGPoint {
                x: f64::from(rect.origin.x),
                y: f64::from(rect.origin.y),
            },
            size: CGSize {
                width: f64::from(rect.size.width),
                height: f64::from(rect.size.height),
            },
        },
        // the middle of the window, no size: a popover without an arrow
        None => CGRect {
            origin: CGPoint {
                x: bounds.size.width / 2.0,
                y: bounds.size.height / 2.0,
            },
            size: CGSize {
                width: 0.0,
                height: 0.0,
            },
        },
    }
}

unsafe fn present(plan: &SharePlan, staged: &[PathBuf]) -> ShareStatus {
    let presenter = presenter();
    if presenter.is_null() {
        return ShareStatus::Failed("there is no window to show the share sheet over".to_string().into());
    }
    let view: *mut AnyObject = msg_send![presenter, view];
    let bounds: CGRect = if view.is_null() {
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize { width: 0.0, height: 0.0 },
        }
    } else {
        msg_send![view, bounds]
    };
    let anchor = rect_of(plan.anchor, bounds);

    let (Some(array_class), Some(url_class)) = (class("NSMutableArray"), class("NSURL")) else {
        return ShareStatus::Failed("Foundation is missing NSMutableArray / NSURL".to_string().into());
    };
    if plan.open_with {
        return open_in(presenter, view, anchor, staged, url_class);
    }
    let items: *mut AnyObject = msg_send![array_class, array];
    let mut files = staged.iter();
    for item in &plan.items {
        match item {
            PlannedItem::File { .. } => {
                let Some(path) = files.next() else {
                    continue;
                };
                let ns_path = NSString::from_str(&path.to_string_lossy());
                let url: *mut AnyObject = msg_send![url_class, fileURLWithPath: &*ns_path];
                if !url.is_null() {
                    let _: () = msg_send![items, addObject: url];
                }
            }
            PlannedItem::Text(text) => {
                let ns = NSString::from_str(text);
                let _: () = msg_send![items, addObject: &*ns];
            }
            PlannedItem::Url(link) => {
                let ns = NSString::from_str(link);
                let url: *mut AnyObject = msg_send![url_class, URLWithString: &*ns];
                if !url.is_null() {
                    let _: () = msg_send![items, addObject: url];
                }
            }
        }
    }

    let Some(sheet_class) = class("UIActivityViewController") else {
        return ShareStatus::Failed("this system has no UIActivityViewController".to_string().into());
    };
    let alloc: *mut AnyObject = msg_send![sheet_class, alloc];
    let nil: *const AnyObject = core::ptr::null();
    let sheet: *mut AnyObject = msg_send![alloc, initWithActivityItems: items, applicationActivities: nil];
    let Some(sheet) = Retained::from_raw(sheet) else {
        return ShareStatus::Failed("the share sheet could not be created".to_string().into());
    };

    // an iPad shows it as a popover, which needs a source (without one UIKit raises an exception and the app ends)
    let popover: *mut AnyObject = msg_send![&*sheet, popoverPresentationController];
    if !popover.is_null() {
        let _: () = msg_send![popover, setSourceView: view];
        let _: () = msg_send![popover, setSourceRect: anchor];
        if plan.anchor.is_none() {
            // UIPopoverArrowDirectionUnknown / none: centred, no arrow
            let _: () = msg_send![popover, setPermittedArrowDirections: 0_usize];
        }
    }
    if !plan.title.is_empty() {
        let title = NSString::from_str(&plan.title);
        // the subject a mail made of the share starts with (`UIActivityViewController` reads `setValue:forKey:@"subject"`)
        let key = NSString::from_str("subject");
        let _: () = msg_send![&*sheet, setValue: &*title, forKey: &*key];
    }
    let _: () = msg_send![
        presenter,
        presentViewController: &*sheet,
        animated: Bool::YES,
        completion: nil
    ];
    ShareStatus::Shown
}

/// "Open with...": the open-in menu of the one file.
unsafe fn open_in(
    presenter: *mut AnyObject,
    view: *mut AnyObject,
    anchor: CGRect,
    staged: &[PathBuf],
    url_class: &AnyClass,
) -> ShareStatus {
    let (Some(controller_class), Some(path)) = (class("UIDocumentInteractionController"), staged.first()) else {
        return ShareStatus::Failed("this system has no UIDocumentInteractionController".to_string().into());
    };
    if view.is_null() {
        return ShareStatus::Failed("there is no view to anchor the open-in menu to".to_string().into());
    }
    let ns_path = NSString::from_str(&path.to_string_lossy());
    let url: *mut AnyObject = msg_send![url_class, fileURLWithPath: &*ns_path];
    let controller: *mut AnyObject = msg_send![controller_class, interactionControllerWithURL: url];
    if controller.is_null() {
        return ShareStatus::Failed("the open-in menu could not be created".to_string().into());
    }
    // the menu is asynchronous and the controller is autoreleased: it lives as long as the view controller it is shown from
    keep_alive(presenter, controller);
    let shown: Bool = msg_send![controller, presentOpenInMenuFromRect: anchor, inView: view, animated: Bool::YES];
    if shown.as_bool() {
        ShareStatus::Shown
    } else {
        ShareStatus::Failed("no app on this iPhone can open this file".to_string().into())
    }
}

/// `objc_setAssociatedObject(owner, key, value, OBJC_ASSOCIATION_RETAIN_NONATOMIC)`: `value` lives as long as `owner` (or until
/// the next call with the same `owner`, which replaces it).
unsafe fn keep_alive(owner: *mut AnyObject, value: *mut AnyObject) {
    extern "C" {
        fn objc_setAssociatedObject(object: *mut AnyObject, key: *const c_void, value: *mut AnyObject, policy: usize);
    }
    // the ADDRESS of a constant is the key
    static KEY: u8 = 0;
    objc_setAssociatedObject(owner, core::ptr::addr_of!(KEY).cast(), value, 1);
}
