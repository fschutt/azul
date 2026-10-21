//! The vsync tick's way from the `CVDisplayLink` thread onto the main thread.
//!
//! The display link fires on a thread of its own, sixty (or more) times a
//! second while a window wants frames. The tick it triggers - deliver the
//! frame a window owes (`mark_views_dirty_on_main`) - has to run on the main
//! thread, because AppKit and the window state live there. [`deliver`] hands
//! one tick over; [`install`] prepares the hand-over once, on the main thread,
//! before the first display link starts.
//!
//! The ticks travel through one version-0 source of the main run loop, in the
//! common modes. The run loop performs a signalled version-0 source at the
//! start of every turn, before it waits for its timers and ports, so a tick
//! runs before the next timer callback whatever the timers do. A block on the
//! main dispatch queue runs only when a turn happens to take the queue's
//! port; a frame timer whose callback takes longer than its interval (an
//! animation step that lays the window out) is due on every turn and wins
//! that race, and the ticks wait until the animation is over: the window lays
//! itself out frame after frame with nothing new on the screen.

use core::ffi::c_void;
use std::sync::{Mutex, OnceLock};

#[repr(C)]
struct CFRunLoopSourceContext {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
    hash: *const c_void,
    schedule: *const c_void,
    cancel: *const c_void,
    perform: extern "C" fn(*mut c_void),
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopSourceCreate(
        allocator: *const c_void,
        order: isize,
        context: *mut CFRunLoopSourceContext,
    ) -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopSourceSignal(source: *mut c_void);
    fn CFRunLoopWakeUp(run_loop: *mut c_void);
    static kCFRunLoopCommonModes: *const c_void;
}

/// A tick waiting for the main thread: the handler and its context.
#[derive(Clone, Copy)]
struct Tick {
    context: usize,
    handler: extern "C" fn(*mut c_void),
}

/// The main run loop's source and the ticks it has to run. Made once, by
/// [`install`], and kept for the life of the process (a display link of any
/// window may fire until the process ends).
struct Ticks {
    /// The `CFRunLoopSourceRef`, as an address: CoreFoundation's source and
    /// run loop calls used here are safe from any thread.
    source: usize,
    /// The main `CFRunLoopRef`, to wake it when it sleeps.
    run_loop: usize,
    /// The ticks delivered and not yet run, oldest first.
    queue: Mutex<Vec<Tick>>,
}

static TICKS: OnceLock<Ticks> = OnceLock::new();

/// The source's perform callback, on the main thread: run every tick
/// delivered so far, oldest first. A tick delivered while these run signals
/// the source again and runs on the next turn.
extern "C" fn perform(_info: *mut c_void) {
    let Some(ticks) = TICKS.get() else {
        return;
    };
    let due = core::mem::take(
        &mut *ticks
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    for tick in due {
        (tick.handler)(tick.context as *mut c_void);
    }
}

/// Prepares the hand-over: the main run loop's tick source. Main thread,
/// before the first [`deliver`]; once is enough, later calls do nothing.
pub fn install() {
    TICKS.get_or_init(|| {
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: core::ptr::null_mut(),
            retain: core::ptr::null(),
            release: core::ptr::null(),
            copy_description: core::ptr::null(),
            equal: core::ptr::null(),
            hash: core::ptr::null(),
            schedule: core::ptr::null(),
            cancel: core::ptr::null(),
            perform,
        };
        // SAFETY: CoreFoundation copies the context; the source is added to
        // the main run loop, which keeps it, and the address is kept here for
        // the life of the process.
        unsafe {
            let run_loop = CFRunLoopGetMain();
            let source = CFRunLoopSourceCreate(core::ptr::null(), 0, &mut context);
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            Ticks {
                source: source as usize,
                run_loop: run_loop as usize,
                queue: Mutex::new(Vec::new()),
            }
        }
    });
}

/// Runs `handler(context)` on the main thread, before the main run loop's
/// next timer callback. Callable from any thread; `context` is the handler's
/// to release.
pub fn deliver(context: *mut c_void, handler: extern "C" fn(*mut c_void)) {
    let Some(ticks) = TICKS.get() else {
        // Not installed: the main dispatch queue still gets it there.
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
        return;
    };
    ticks
        .queue
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(Tick {
            context: context as usize,
            handler,
        });
    // SAFETY: both calls are documented as safe from any thread; the source
    // and the run loop live for the rest of the process.
    unsafe {
        CFRunLoopSourceSignal(ticks.source as *mut c_void);
        CFRunLoopWakeUp(ticks.run_loop as *mut c_void);
    }
}
