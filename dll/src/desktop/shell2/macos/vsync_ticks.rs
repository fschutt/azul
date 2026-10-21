//! The vsync tick's way from the `CVDisplayLink` thread onto the main thread.
//!
//! The display link fires on a thread of its own, sixty (or more) times a
//! second while a window wants frames. The tick it triggers - deliver the
//! frame a window owes (`mark_views_dirty_on_main`) - has to run on the main
//! thread, because AppKit and the window state live there. [`deliver`] hands
//! one tick over; [`install`] prepares the hand-over once, on the main thread,
//! before the first display link starts.

use core::ffi::c_void;

/// Prepares the hand-over. Main thread, before the first [`deliver`].
pub fn install() {}

/// Runs `handler(context)` on the main thread, soon. Callable from any
/// thread; `context` is the handler's to release.
pub fn deliver(context: *mut c_void, handler: extern "C" fn(*mut c_void)) {
    extern "C" {
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(
            queue: *const c_void,
            context: *mut c_void,
            work: extern "C" fn(*mut c_void),
        );
    }
    // SAFETY: libdispatch's main queue lives for the whole process.
    unsafe {
        dispatch_async_f(core::ptr::addr_of!(_dispatch_main_q), context, handler);
    }
}
