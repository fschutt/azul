//! A vsync tick reaches the main thread while an animation's frames overrun
//! their interval.
//!
//! An animation is stepped by a timer at the frame interval, and each step
//! lays the window out again. When a step takes longer than the interval,
//! the timer is due again the moment its callback returns. The frame the
//! step produced is only shown once the display link's tick
//! (`vsync_ticks::deliver`) has run on the main thread. The tick must get
//! there before the next step: a tick that waits behind further steps lets
//! the window lay itself out again and again with nothing on the screen,
//! which is how AzDrive's ribbon content opened in two or three visible
//! frames.
//!
//! The main thread here runs the run loop the way azul's macOS event loop
//! does: zero-timeout `nextEventMatchingMask:` polls, then one blocking
//! `runMode:beforeDate:`. A one-shot timer stands for the animation's step
//! (20 ms of work, armed again for one frame interval after it began, so it
//! is due at once), a repeating 16 ms timer for another frame-paced timer of
//! the window. A thread delivers a tick every 16.7 ms and notes how many
//! steps had begun; each tick must run with at most one more step begun.
//!
//! A test of its own (`harness = false`): the run loop and the queue under
//! test are the MAIN thread's, which libtest's worker threads are not.

#[cfg(target_os = "macos")]
mod macos {
    use core::ffi::c_void;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    use azul::desktop::shell2::macos::vsync_ticks;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEventMask};

    /// One frame at 60 Hz, in seconds.
    const FRAME: f64 = 1.0 / 60.0;
    /// How long one animation step works, in ms: more than a frame.
    const STEP_MS: u64 = 20;
    /// How many ticks the thread delivers.
    const TICKS: usize = 60;

    /// Steps begun so far.
    static STEPS: AtomicUsize = AtomicUsize::new(0);
    /// Per tick: the steps begun when it was delivered (`usize::MAX`: not yet).
    static SENT: [AtomicUsize; TICKS] = [const { AtomicUsize::new(usize::MAX) }; TICKS];
    /// Per tick: the steps begun when it ran (`usize::MAX`: not yet).
    static RAN: [AtomicUsize; TICKS] = [const { AtomicUsize::new(usize::MAX) }; TICKS];

    #[repr(C)]
    struct CFRunLoopTimerContext {
        version: isize,
        info: *mut c_void,
        retain: *const c_void,
        release: *const c_void,
        copy_description: *const c_void,
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRunLoopGetMain() -> *mut c_void;
        fn CFAbsoluteTimeGetCurrent() -> f64;
        fn CFRunLoopTimerCreate(
            allocator: *const c_void,
            fire_date: f64,
            interval: f64,
            flags: usize,
            order: isize,
            callout: extern "C" fn(*mut c_void, *mut c_void),
            context: *mut CFRunLoopTimerContext,
        ) -> *mut c_void;
        fn CFRunLoopAddTimer(run_loop: *mut c_void, timer: *mut c_void, mode: *const c_void);
        fn CFRelease(object: *const c_void);
        static kCFRunLoopCommonModes: *const c_void;
    }

    fn add_timer(fire_in: f64, interval: f64, callout: extern "C" fn(*mut c_void, *mut c_void)) {
        let mut context = CFRunLoopTimerContext {
            version: 0,
            info: core::ptr::null_mut(),
            retain: core::ptr::null(),
            release: core::ptr::null(),
            copy_description: core::ptr::null(),
        };
        // SAFETY: plain CoreFoundation calls on the main thread; the run loop
        // keeps the timer, so ours is released at once.
        unsafe {
            let timer = CFRunLoopTimerCreate(
                core::ptr::null(),
                CFAbsoluteTimeGetCurrent() + fire_in,
                interval,
                0,
                0,
                callout,
                &mut context,
            );
            CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
            CFRelease(timer);
        }
    }

    /// The animation's step: works for longer than a frame, then is due again
    /// one frame after it began - at once.
    extern "C" fn step(_timer: *mut c_void, _info: *mut c_void) {
        let began = Instant::now();
        STEPS.fetch_add(1, Ordering::SeqCst);
        while began.elapsed() < Duration::from_millis(STEP_MS) {
            core::hint::spin_loop();
        }
        let next = FRAME - began.elapsed().as_secs_f64();
        add_timer(next, 0.0, step);
    }

    extern "C" fn other_frame_timer(_timer: *mut c_void, _info: *mut c_void) {}

    extern "C" fn tick(context: *mut c_void) {
        let index = context as usize;
        RAN[index].store(STEPS.load(Ordering::SeqCst), Ordering::SeqCst);
    }

    pub fn run() {
        let mtm = MainThreadMarker::new().expect("a harness = false test runs on the main thread");
        let app = NSApplication::sharedApplication(mtm);
        vsync_ticks::install();
        add_timer(0.0, 0.0, step);
        add_timer(0.0, 0.016, other_frame_timer);

        let sender = std::thread::spawn(|| {
            for (index, sent) in SENT.iter().enumerate() {
                std::thread::sleep(Duration::from_micros(16_667));
                sent.store(STEPS.load(Ordering::SeqCst), Ordering::SeqCst);
                vsync_ticks::deliver(index as *mut c_void, tick);
            }
        });

        // azul's macOS loop: the event drain's polls, then one blocking run.
        let deadline = Instant::now() + Duration::from_millis(TICKS as u64 * 17 + 400);
        while Instant::now() < deadline && RAN[TICKS - 1].load(Ordering::SeqCst) == usize::MAX {
            objc2::rc::autoreleasepool(|_| unsafe {
                for _ in 0..3 {
                    if let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                        NSEventMask::Any,
                        None,
                        objc2_foundation::NSDefaultRunLoopMode,
                        true,
                    ) {
                        app.sendEvent(&event);
                    }
                }
                let soon = objc2_foundation::NSDate::dateWithTimeIntervalSinceNow(0.1);
                objc2_foundation::NSRunLoop::currentRunLoop()
                    .runMode_beforeDate(objc2_foundation::NSDefaultRunLoopMode, &soon);
            });
        }
        sender.join().expect("the sender thread");

        let mut late = Vec::new();
        for index in 0..TICKS {
            let sent = SENT[index].load(Ordering::SeqCst);
            let ran = RAN[index].load(Ordering::SeqCst);
            if ran == usize::MAX {
                late.push(format!("tick {index} never ran"));
            } else if ran.saturating_sub(sent) > 1 {
                late.push(format!(
                    "tick {index} ran {} steps after it was delivered",
                    ran - sent
                ));
            }
        }
        println!(
            "[vsync_ticks] {TICKS} ticks, {} animation steps, {} late",
            STEPS.load(Ordering::SeqCst),
            late.len()
        );
        assert!(
            late.is_empty(),
            "a vsync tick must reach the main thread before the animation's next step - {}",
            late.join("; ")
        );
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    macos::run();
}
