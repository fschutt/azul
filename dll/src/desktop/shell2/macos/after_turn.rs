//! Work that runs once, right after the current turn of a run loop.
//!
//! A frame's accessibility update used to reach the platform adapter inside
//! `drawRect:` and again after every NSEvent of a burst: accesskit's macOS
//! adapter diffs the update and posts its notifications there, per changed
//! node, on the thread that is drawing. An [`AfterTurn`] collects the windows
//! that have an update parked and hands each one over ONCE, at the start of
//! the run loop's next turn - after the draw call returned, however many
//! layout passes and events asked for it in between (each pass folds its
//! patch into the one parked update, `A11yManager::publish`).
//!
//! The hand-over is a version-0 source of the run loop, in the common modes
//! (live resize and menu tracking included): the run loop performs a
//! signalled version-0 source at the start of a turn, before timers, so a
//! request made in a draw call runs as soon as that call is over.

use core::ffi::c_void;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex, OnceLock,
};

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

/// One run loop's after-turn hand-over: the tokens requested since its last
/// perform, and what to do with each.
pub struct AfterTurn {
    /// The `CFRunLoopSourceRef`, as an address (set once, right after the
    /// source exists). CoreFoundation's signal and wake calls are safe from
    /// any thread.
    source: AtomicUsize,
    /// The `CFRunLoopRef` the source is on, to wake it when it sleeps.
    run_loop: usize,
    /// Tokens requested and not handed over yet, each once, oldest first.
    due: Mutex<Vec<usize>>,
    /// Runs on the run loop's thread, once per due token.
    deliver: fn(usize),
}

extern "C" fn perform(info: *mut c_void) {
    if info.is_null() {
        return;
    }
    // SAFETY: `info` is the leaked `AfterTurn` of `AfterTurn::on`, which
    // lives for the rest of the process.
    let after_turn = unsafe { &*info.cast::<AfterTurn>() };
    let due = core::mem::take(
        &mut *after_turn
            .due
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    for token in due {
        (after_turn.deliver)(token);
    }
}

impl AfterTurn {
    /// An after-turn hand-over on `run_loop` (a `CFRunLoopRef`), in the
    /// common modes. Kept for the life of the process.
    ///
    /// # Safety
    ///
    /// `run_loop` is a live run loop; `deliver` then runs on its thread.
    pub unsafe fn on(run_loop: *mut c_void, deliver: fn(usize)) -> &'static Self {
        let after_turn: &'static Self = Box::leak(Box::new(Self {
            source: AtomicUsize::new(0),
            run_loop: run_loop as usize,
            due: Mutex::new(Vec::new()),
            deliver,
        }));
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: (after_turn as *const Self).cast_mut().cast(),
            retain: core::ptr::null(),
            release: core::ptr::null(),
            copy_description: core::ptr::null(),
            equal: core::ptr::null(),
            hash: core::ptr::null(),
            schedule: core::ptr::null(),
            cancel: core::ptr::null(),
            perform,
        };
        // SAFETY: CoreFoundation copies the context; the run loop keeps the
        // source, and its info (the leaked hand-over) never goes away.
        let source = CFRunLoopSourceCreate(core::ptr::null(), 0, &mut context);
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
        after_turn.source.store(source as usize, Ordering::Release);
        after_turn
    }

    /// Hand `token` over at the start of the run loop's next turn - once,
    /// however often it is requested before that.
    pub fn request(&self, token: usize) {
        {
            let mut due = self
                .due
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if due.contains(&token) {
                return;
            }
            due.push(token);
        }
        let source = self.source.load(Ordering::Acquire);
        if source == 0 {
            return;
        }
        // SAFETY: both calls are documented as safe from any thread; the
        // source and the run loop live for the rest of the process.
        unsafe {
            CFRunLoopSourceSignal(source as *mut c_void);
            CFRunLoopWakeUp(self.run_loop as *mut c_void);
        }
    }
}

/// The main run loop's hand-over of parked accessibility updates (a token is
/// a `MacOSWindow` address). Made on the main thread by the first request.
pub fn a11y_delivery() -> &'static AfterTurn {
    static A11Y: OnceLock<&'static AfterTurn> = OnceLock::new();
    // SAFETY: the main run loop lives for the whole process, and the
    // delivery runs on the main thread, where the windows live.
    A11Y.get_or_init(|| unsafe { AfterTurn::on(CFRunLoopGetMain(), super::deliver_parked_a11y) })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
        static kCFRunLoopDefaultMode: *const c_void;
    }

    static DELIVERED: AtomicUsize = AtomicUsize::new(0);
    static DRAWING: AtomicUsize = AtomicUsize::new(0);
    static DELIVERED_IN_DRAW: AtomicUsize = AtomicUsize::new(0);

    fn count(_token: usize) {
        DELIVERED.fetch_add(1, Ordering::SeqCst);
        if DRAWING.load(Ordering::SeqCst) != 0 {
            DELIVERED_IN_DRAW.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// A window that parks its update on every one of 50 layout passes and
    /// events inside one draw hands it over once, after the draw returned.
    #[test]
    fn requests_made_in_a_draw_are_handed_over_once_after_it() {
        // SAFETY: the test thread's own run loop, run below on this thread.
        let after_turn = unsafe { AfterTurn::on(CFRunLoopGetCurrent(), count) };
        let window = 0x1234usize;

        DRAWING.store(1, Ordering::SeqCst);
        for _ in 0..50 {
            after_turn.request(window);
        }
        DRAWING.store(0, Ordering::SeqCst);
        assert_eq!(
            DELIVERED.load(Ordering::SeqCst),
            0,
            "nothing is handed over while the draw runs"
        );

        // SAFETY: plain CoreFoundation calls on this thread's run loop.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, 1);
        }
        assert_eq!(
            DELIVERED.load(Ordering::SeqCst),
            1,
            "50 requests of one window in one turn are one hand-over"
        );
        assert_eq!(DELIVERED_IN_DRAW.load(Ordering::SeqCst), 0);

        // A later turn without a request hands over nothing.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.02, 1);
        }
        assert_eq!(DELIVERED.load(Ordering::SeqCst), 1);
    }
}
