# Timer and Thread Management Research

## Executive Summary

After removing the unused `process.rs` module, we discovered that **timer and thread management from callbacks is completely broken on all 4 platforms**. The `CallCallbacksResult` structure contains `timers`, `threads`, `timers_removed`, and `threads_removed` fields that are never processed.

**Solution**: Extend `PlatformWindowV2` trait with native timer/thread management methods.

---

## Native Timer/Thread APIs by Platform

### 1. Windows (Win32)

#### Timer Management
**API**: `SetTimer()` / `KillTimer()`

```c
// From winuser.h
UINT_PTR SetTimer(
    HWND      hWnd,         // Window handle
    UINT_PTR  nIDEvent,     // Timer identifier
    UINT      uElapse,      // Timeout value in milliseconds
    TIMERPROC lpTimerFunc   // Timer procedure (NULL for message-based)
);

BOOL KillTimer(
    HWND     hWnd,          // Window handle
    UINT_PTR uIDEvent       // Timer identifier
);
```

**How it works**:
- `SetTimer()` creates a timer that posts `WM_TIMER` messages to the window's message queue
- `wparam` contains the timer ID
- Timers are identified by UINT_PTR (can be any number)
- No callback function needed - messages go through normal `WndProc`

**Old Implementation** (REFACTORING/shell/win32/mod.rs lines 1089-1109):
```rust
pub fn start_stop_timers(
    &mut self,
    added: FastHashMap<TimerId, Timer>,
    removed: FastBTreeSet<TimerId>,
) {
    use winapi::um::winuser::{KillTimer, SetTimer};

    for (id, timer) in added {
        let res = unsafe {
            SetTimer(
                self.hwnd,
                id.id,                                              // Timer ID
                timer.tick_millis().min(u32::MAX as u64) as u32,   // Interval
                None,                                               // No callback
            )
        };
        self.internal.timers.insert(id, timer);
        self.timers.insert(id, res);
    }

    for id in removed {
        if let Some(_) = self.internal.timers.remove(&id) {
            if let Some(handle) = self.timers.remove(&id) {
                unsafe { KillTimer(self.hwnd, handle) };
            }
        }
    }
}
```

**WM_TIMER Handler** (REFACTORING/shell/win32/mod.rs lines 2805-2950):
```rust
WM_TIMER => {
    match wparam {
        AZ_THREAD_TICK => {
            // Thread polling timer (16ms)
            ret = process_threads(...);
        }
        id => {
            // User timer with ID "id"
            ret = process_timer(id, ...);
        }
    }
}
```

#### Thread Management
**Mechanism**: 16ms polling timer using `SetTimer()`

**Old Implementation** (REFACTORING/shell/win32/mod.rs lines 1111-1135):
```rust
pub fn start_stop_threads(
    &mut self,
    mut added: FastHashMap<ThreadId, Thread>,
    removed: FastBTreeSet<ThreadId>,
) {
    use winapi::um::winuser::{KillTimer, SetTimer};

    self.internal.threads.append(&mut added);
    self.internal.threads.retain(|r, _| !removed.contains(r));

    if self.internal.threads.is_empty() {
        // No threads - stop polling timer
        if let Some(thread_tick) = self.thread_timer_running {
            unsafe { KillTimer(self.hwnd, thread_tick) };
        }
        self.thread_timer_running = None;
    } else if !self.internal.threads.is_empty() && self.thread_timer_running.is_none() {
        // Have threads - start 16ms polling timer
        let res = unsafe { SetTimer(self.hwnd, AZ_THREAD_TICK, 16, None) };
        self.thread_timer_running = Some(res);
    }
}
```

**Constants**:
```rust
const AZ_THREAD_TICK: u32 = WM_APP + ...; // Reserved timer ID for thread polling
```

**Summary**:
- ✅ Simple message-based API
- ✅ Timer IDs are just integers
- ✅ Thread polling uses dedicated 16ms timer
- ✅ Already implemented in current code (lines 415-460)

---

### 2. macOS (Cocoa/NSTimer)

#### Timer Management
**API**: `NSTimer::scheduledTimerWithTimeInterval`

```objc
// From NSTimer.h
+ (NSTimer *)scheduledTimerWithTimeInterval:(NSTimeInterval)ti
                                     target:(id)aTarget
                                   selector:(SEL)aSelector
                                   userInfo:(id)userInfo
                                    repeats:(BOOL)yesOrNo;
```

**Rust binding** (using objc2):
```rust
let timer: Retained<NSTimer> = unsafe {
    msg_send_id![
        NSTimer::class(),
        scheduledTimerWithTimeInterval: interval,  // f64 seconds
        target: target,                            // NSObject to call
        selector: sel!(timerFired:),               // Method to invoke
        userInfo: nil,
        repeats: YES
    ]
};
```

**Current Implementation** (dll/src/desktop/shell2/macos/mod.rs lines 2046-2075):
```rust
pub fn start_thread_tick_timer(&mut self) {
    if self.thread_timer_running.is_some() {
        return; // Already running
    }

    let interval: f64 = 0.016; // 16ms

    // Using scheduledTimerWithTimeInterval for simplicity
    let timer: Retained<NSTimer> = unsafe {
        let target = &*self.ns_window;
        msg_send_id![
            NSTimer::class(),
            scheduledTimerWithTimeInterval: interval,
            target: target,
            selector: sel!(threadTimerFired:),
            userInfo: nil,
            repeats: YES
        ]
    };

    self.thread_timer_running = Some(timer);
}

pub fn stop_thread_tick_timer(&mut self) {
    if let Some(timer) = self.thread_timer_running.take() {
        unsafe {
            let _: () = msg_send![&*timer, invalidate];
        }
    }
}
```

**Storage**:
```rust
/// Active timers (TimerId -> NSTimer object)
timers: std::collections::HashMap<usize, Retained<objc2_foundation::NSTimer>>,

/// Thread polling timer (16ms interval)
thread_timer_running: Option<Retained<objc2_foundation::NSTimer>>,
```

**How it works**:
- `scheduledTimerWithTimeInterval` creates timer and adds to run loop automatically
- Timer calls selector method on target object
- Must call `[timer invalidate]` to stop
- Timer is retained until invalidated

**Summary**:
- ✅ High-level API (auto-scheduled)
- ⚠️ Requires Objective-C method dispatch
- ⚠️ Must store `Retained<NSTimer>` to keep alive
- ✅ Thread polling structure already exists (lines 2046-2075)
- ❌ Never called from callback processing

---

### 3. X11 (Linux)

#### Timer Management
**Problem**: X11 has **no native timer API**

**Solutions**:

**Option A: POSIX timer_create()** (Modern, requires librt)
```c
#include <signal.h>
#include <time.h>

timer_t timerid;
struct sigevent sev;
struct itimerspec its;

// Create timer
sev.sigev_notify = SIGEV_SIGNAL;
sev.sigev_signo = SIGRTMIN;
sev.sigev_value.sival_ptr = &timerid;
timer_create(CLOCK_REALTIME, &sev, &timerid);

// Start timer
its.it_value.tv_sec = 1;
its.it_value.tv_nsec = 0;
its.it_interval.tv_sec = 1;
its.it_interval.tv_nsec = 0;
timer_settime(timerid, 0, &its, NULL);
```

**Option B: select() with timeout** (Portable, simple)
```c
// In event loop
fd_set readfds;
struct timeval timeout;

FD_ZERO(&readfds);
FD_SET(x11_connection_fd, &readfds);

// Calculate timeout to next timer
timeout.tv_sec = 0;
timeout.tv_usec = 16000; // 16ms

select(x11_connection_fd + 1, &readfds, NULL, NULL, &timeout);

if (FD_ISSET(x11_connection_fd, &readfds)) {
    // X11 event available
    XNextEvent(display, &event);
}

// Check which timers expired
check_timer_expiration(current_time);
```

**Option C: Dedicated timer thread** (Complex, more overhead)
```rust
struct TimerThread {
    thread: JoinHandle<()>,
    timers: Arc<Mutex<BTreeMap<TimerId, Timer>>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl TimerThread {
    fn spawn() -> Self {
        let timers = Arc::new(Mutex::new(BTreeMap::new()));
        let timers_clone = timers.clone();
        
        let thread = std::thread::spawn(move || {
            loop {
                let next_timeout = calculate_next_timeout(&timers_clone);
                std::thread::sleep(next_timeout);
                
                // Send X11 ClientMessage to wake event loop
                send_timer_event_to_x11();
            }
        });
        
        Self { thread, timers, waker }
    }
}
```

**Recommended: Option B (select with timeout)**
- ✅ Portable (POSIX)
- ✅ No additional dependencies
- ✅ Integrates with X11 event loop
- ✅ Low overhead
- ❌ Requires refactoring event loop

**Current Status** (dll/src/desktop/shell2/linux/x11/events.rs lines 203-210):
```rust
// Handle timers and threads
if result.timers.is_some()
    || result.timers_removed.is_some()
    || result.threads.is_some()
    || result.threads_removed.is_some()
{
    // TODO: Implement timer/thread management for X11
    event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
}
```

**Summary**:
- ❌ No native X11 timer API
- ✅ Can use select() with timeout
- ⚠️ Requires event loop refactoring
- ❌ Currently has TODO placeholder

---

### 4. Wayland (Linux)

#### Timer Management
**Problem**: Wayland protocol has **no native timer API**

**Solutions**:

**Option A: timerfd (Linux-specific, clean)**
```c
#include <sys/timerfd.h>

int timerfd = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK);

struct itimerspec spec;
spec.it_value.tv_sec = 1;
spec.it_value.tv_nsec = 0;
spec.it_interval.tv_sec = 1;
spec.it_interval.tv_nsec = 0;

timerfd_settime(timerfd, 0, &spec, NULL);

// In event loop with epoll/poll:
struct epoll_event ev;
ev.events = EPOLLIN;
ev.data.fd = timerfd;
epoll_ctl(epollfd, EPOLL_CTL_ADD, timerfd, &ev);

// When timer fires:
uint64_t expirations;
read(timerfd, &expirations, sizeof(expirations));
```

**Option B: POSIX timer_create() with eventfd**
```c
int eventfd = eventfd(0, EFD_NONBLOCK);

struct sigevent sev;
sev.sigev_notify = SIGEV_THREAD;
sev.sigev_value.sival_ptr = &eventfd;
sev.sigev_notify_function = timer_callback;

timer_t timerid;
timer_create(CLOCK_MONOTONIC, &sev, &timerid);
```

**Option C: Share implementation with X11** (select/poll)
```rust
// Use same approach as X11
// Calculate timeout to next timer in event loop
```

**Recommended: Option A (timerfd) or C (shared select)**
- ✅ timerfd integrates cleanly with Wayland's event loop
- ✅ Can share event loop infrastructure with X11
- ⚠️ timerfd is Linux-specific (but so is Wayland)

**Current Status**: 
```
❌ NO callback result processing at all
❌ NO timer management
❌ NO thread management
```

**Summary**:
- ❌ No native Wayland timer API
- ✅ Can use timerfd (clean, Linux-specific)
- ✅ Can share select() approach with X11
- ❌ Currently completely missing callback processing

---

## CallCallbacksResult Structure

```rust
// From azul_layout::callbacks
pub struct CallCallbacksResult {
    pub callbacks_update_screen: Update,
    pub modified_window_state: Option<WindowState>,
    
    // ❌ NEVER PROCESSED ON ANY PLATFORM:
    pub timers: Option<FastHashMap<TimerId, Timer>>,              
    pub threads: Option<FastHashMap<ThreadId, Thread>>,           
    pub timers_removed: Option<FastBTreeSet<TimerId>>,            
    pub threads_removed: Option<FastBTreeSet<ThreadId>>,          
    
    pub windows_created: Vec<WindowCreateOptions>,
    pub update_focused_node: Option<...>,
    pub images_changed: Option<...>,
    // ... other fields
}
```

**What should happen**:
1. Callback returns `CallCallbacksResult` with `timers: Some({...})`
2. Platform extracts timer IDs and intervals
3. Platform calls native timer API (`SetTimer`, `NSTimer`, etc.)
4. Timer fires → platform invokes timer callback
5. Repeat cycle

**What actually happens**: ❌ Fields are ignored, timers never start

---

## Proposed Solution: Extend PlatformWindowV2 Trait

### Add New Required Methods

```rust
pub trait PlatformWindowV2 {
    // ... existing methods ...

    // =========================================================================
    // Timer Management (Platform-Specific Implementation Required)
    // =========================================================================

    /// Start a timer with the given ID and interval.
    ///
    /// When the timer fires, the platform should invoke the timer callback
    /// through the normal event processing system.
    ///
    /// ## Parameters
    /// * `timer_id` - Unique timer identifier (from TimerId.id)
    /// * `timer` - Timer configuration with interval and callback info
    fn start_timer(&mut self, timer_id: usize, timer: Timer);

    /// Stop a timer with the given ID.
    ///
    /// ## Parameters
    /// * `timer_id` - Timer identifier to stop
    fn stop_timer(&mut self, timer_id: usize);

    // =========================================================================
    // Thread Management (Platform-Specific Implementation Required)
    // =========================================================================

    /// Start the thread polling timer (typically 16ms interval).
    ///
    /// This timer should poll all active threads to check for completed work.
    /// Platforms typically use a dedicated timer ID for this (e.g., 0xFFFF on Windows).
    fn start_thread_poll_timer(&mut self);

    /// Stop the thread polling timer.
    ///
    /// Called when the last thread is removed from the thread pool.
    fn stop_thread_poll_timer(&mut self);

    /// Add threads to the thread pool.
    ///
    /// ## Parameters
    /// * `threads` - Threads to add to the pool
    fn add_threads(&mut self, threads: HashMap<ThreadId, Thread>);

    /// Remove threads from the thread pool.
    ///
    /// ## Parameters  
    /// * `thread_ids` - Thread IDs to remove
    fn remove_threads(&mut self, thread_ids: &BTreeSet<ThreadId>);
}
```

### Update process_callback_result_v2() (Default Implementation)

```rust
// In dll/src/desktop/shell2/common/event_v2.rs
impl<T: PlatformWindowV2> PlatformWindowV2 for T {
    fn process_callback_result_v2(&mut self, result: &CallCallbacksResult) -> ProcessEventResult {
        let mut event_result = ProcessEventResult::DoNothing;

        // ... existing window state handling ...

        // ✅ NEW: Process timers
        if let Some(ref timers) = result.timers {
            for (timer_id, timer) in timers.iter() {
                self.start_timer(timer_id.id, timer.clone());
            }
        }

        if let Some(ref timers_removed) = result.timers_removed {
            for timer_id in timers_removed.iter() {
                self.stop_timer(timer_id.id);
            }
        }

        // ✅ NEW: Process threads
        if let Some(ref threads) = result.threads {
            self.add_threads(threads.clone());
            self.start_thread_poll_timer();
        }

        if let Some(ref threads_removed) = result.threads_removed {
            self.remove_threads(threads_removed);
            
            // Stop polling timer if no threads remain
            if let Some(layout_window) = self.get_layout_window() {
                if layout_window.threads.is_empty() {
                    self.stop_thread_poll_timer();
                }
            }
        }

        // ... existing Update processing ...

        event_result
    }
}
```

---

## Platform Implementation Guide

### Windows Implementation

```rust
// In dll/src/desktop/shell2/windows/mod.rs
impl PlatformWindowV2 for WindowsWindowV2 {
    fn start_timer(&mut self, timer_id: usize, timer: Timer) {
        use winapi::um::winuser::SetTimer;
        
        let interval_ms = timer.tick_millis().min(u32::MAX as u64) as u32;
        
        unsafe {
            SetTimer(
                self.hwnd,
                timer_id as UINT_PTR,
                interval_ms,
                ptr::null(),
            );
        };
        
        // Store timer in layout window
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.insert(
                azul_core::callbacks::TimerId { id: timer_id },
                timer
            );
        }
    }

    fn stop_timer(&mut self, timer_id: usize) {
        use winapi::um::winuser::KillTimer;
        
        unsafe {
            KillTimer(self.hwnd, timer_id as UINT_PTR);
        };
        
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.remove(&azul_core::callbacks::TimerId { id: timer_id });
        }
    }

    fn start_thread_poll_timer(&mut self) {
        use winapi::um::winuser::SetTimer;
        
        const THREAD_TIMER_ID: UINT_PTR = 0xFFFF;
        
        if self.thread_timer_running {
            return; // Already running
        }
        
        unsafe {
            SetTimer(self.hwnd, THREAD_TIMER_ID, 16, ptr::null());
        };
        
        self.thread_timer_running = true;
    }

    fn stop_thread_poll_timer(&mut self) {
        use winapi::um::winuser::KillTimer;
        
        const THREAD_TIMER_ID: UINT_PTR = 0xFFFF;
        
        if !self.thread_timer_running {
            return;
        }
        
        unsafe {
            KillTimer(self.hwnd, THREAD_TIMER_ID);
        };
        
        self.thread_timer_running = false;
    }

    fn add_threads(&mut self, threads: HashMap<ThreadId, Thread>) {
        if let Some(layout_window) = self.get_layout_window_mut() {
            for (id, thread) in threads {
                layout_window.threads.insert(id, thread);
            }
        }
    }

    fn remove_threads(&mut self, thread_ids: &BTreeSet<ThreadId>) {
        if let Some(layout_window) = self.get_layout_window_mut() {
            for id in thread_ids {
                layout_window.threads.remove(id);
            }
        }
    }
}

// Update WM_TIMER handler
WM_TIMER => {
    let timer_id = wparam as usize;
    
    if timer_id == 0xFFFF {
        // Thread polling timer
        if let Some(layout_window) = window.get_layout_window_mut() {
            if !layout_window.threads.is_empty() {
                window.mark_frame_needs_regeneration();
                // Threads will be polled during next frame generation
            }
        }
    } else {
        // User timer
        let current_time = std::time::Instant::now();
        
        if let Some(layout_window) = window.get_layout_window_mut() {
            let expired_timers = layout_window.tick_timers(current_time);
            
            if !expired_timers.is_empty() {
                window.mark_frame_needs_regeneration();
                // Timer callbacks will be invoked during next event processing
            }
        }
    }
}

// Update WM_COMMAND handler (menu callbacks)
WM_COMMAND => {
    // ... existing menu callback invocation ...
    
    let callback_result = layout_window.invoke_single_callback(...);
    
    // ✅ NEW: Process callback results (uses trait default implementation)
    let event_result = window.process_callback_result_v2(&callback_result);
    
    // ... existing Update handling ...
}
```

### macOS Implementation

```rust
// In dll/src/desktop/shell2/macos/mod.rs
impl PlatformWindowV2 for MacOSWindowV2 {
    fn start_timer(&mut self, timer_id: usize, timer: Timer) {
        let interval: f64 = timer.tick_millis() as f64 / 1000.0;
        
        let ns_timer: Retained<NSTimer> = unsafe {
            msg_send_id![
                NSTimer::class(),
                scheduledTimerWithTimeInterval: interval,
                target: &*self.ns_window,
                selector: sel!(userTimerFired:),
                userInfo: NSNumber::numberWithUnsignedInteger(timer_id),
                repeats: YES
            ]
        };
        
        self.timers.insert(timer_id, ns_timer);
        
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.insert(
                azul_core::callbacks::TimerId { id: timer_id },
                timer
            );
        }
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(timer) = self.timers.remove(&timer_id) {
            unsafe {
                let _: () = msg_send![&*timer, invalidate];
            }
        }
        
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.remove(&azul_core::callbacks::TimerId { id: timer_id });
        }
    }

    fn start_thread_poll_timer(&mut self) {
        if self.thread_timer_running.is_some() {
            return;
        }
        
        let interval: f64 = 0.016; // 16ms
        
        let timer: Retained<NSTimer> = unsafe {
            msg_send_id![
                NSTimer::class(),
                scheduledTimerWithTimeInterval: interval,
                target: &*self.ns_window,
                selector: sel!(threadTimerFired:),
                userInfo: nil,
                repeats: YES
            ]
        };
        
        self.thread_timer_running = Some(timer);
    }

    fn stop_thread_poll_timer(&mut self) {
        if let Some(timer) = self.thread_timer_running.take() {
            unsafe {
                let _: () = msg_send![&*timer, invalidate];
            }
        }
    }

    fn add_threads(&mut self, threads: HashMap<ThreadId, Thread>) {
        if let Some(layout_window) = self.get_layout_window_mut() {
            for (id, thread) in threads {
                layout_window.threads.insert(id, thread);
            }
        }
    }

    fn remove_threads(&mut self, thread_ids: &BTreeSet<ThreadId>) {
        if let Some(layout_window) = self.get_layout_window_mut() {
            for id in thread_ids {
                layout_window.threads.remove(id);
            }
        }
    }
}

// Add timer fired selectors
#[allow(non_snake_case)]
extern "C" fn userTimerFired(this: &Object, _cmd: Sel, timer: *mut Object) {
    unsafe {
        let timer_id: usize = msg_send![timer, userInfo];
        let window: *mut MacOSWindowV2 = *this.get_ivar("window_ptr");
        
        if let Some(window) = window.as_mut() {
            if let Some(layout_window) = window.get_layout_window_mut() {
                let current_time = std::time::Instant::now();
                let expired_timers = layout_window.tick_timers(current_time);
                
                if !expired_timers.is_empty() {
                    window.mark_frame_needs_regeneration();
                }
            }
        }
    }
}

#[allow(non_snake_case)]
extern "C" fn threadTimerFired(this: &Object, _cmd: Sel, _timer: *mut Object) {
    unsafe {
        let window: *mut MacOSWindowV2 = *this.get_ivar("window_ptr");
        
        if let Some(window) = window.as_mut() {
            if let Some(layout_window) = window.get_layout_window_mut() {
                if !layout_window.threads.is_empty() {
                    window.mark_frame_needs_regeneration();
                }
            }
        }
    }
}

// Update menu callback handler
let callback_result = layout_window.invoke_single_callback(...);

// ✅ NEW: Process callback results (uses trait default implementation)
let event_result = self.process_callback_result_v2(&callback_result);
```

### X11 Implementation (select-based)

```rust
// In dll/src/desktop/shell2/linux/x11/mod.rs

struct X11TimerManager {
    timers: BTreeMap<usize, (Timer, std::time::Instant)>, // (timer, next_fire_time)
}

impl X11TimerManager {
    fn calculate_next_timeout(&self) -> Option<Duration> {
        let now = std::time::Instant::now();
        self.timers
            .values()
            .map(|(_, fire_time)| {
                if *fire_time > now {
                    *fire_time - now
                } else {
                    Duration::ZERO
                }
            })
            .min()
    }
    
    fn get_expired_timers(&mut self) -> Vec<usize> {
        let now = std::time::Instant::now();
        let mut expired = Vec::new();
        
        for (id, (timer, fire_time)) in &mut self.timers {
            if *fire_time <= now {
                expired.push(*id);
                *fire_time = now + Duration::from_millis(timer.tick_millis());
            }
        }
        
        expired
    }
}

impl PlatformWindowV2 for X11WindowV2 {
    fn start_timer(&mut self, timer_id: usize, timer: Timer) {
        let fire_time = std::time::Instant::now() 
            + Duration::from_millis(timer.tick_millis());
        
        self.timer_manager.timers.insert(timer_id, (timer.clone(), fire_time));
        
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.insert(
                azul_core::callbacks::TimerId { id: timer_id },
                timer
            );
        }
    }

    fn stop_timer(&mut self, timer_id: usize) {
        self.timer_manager.timers.remove(&timer_id);
        
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.remove(&azul_core::callbacks::TimerId { id: timer_id });
        }
    }

    // ... thread management similar to Windows ...
}

// Update event loop
pub fn run_event_loop() {
    loop {
        // Calculate timeout to next timer
        let timeout = timer_manager.calculate_next_timeout()
            .unwrap_or(Duration::from_millis(16)); // Default 16ms for threads
        
        // Wait for X11 events with timeout
        let has_event = wait_for_x11_event_with_timeout(
            display,
            connection_fd,
            timeout
        );
        
        if has_event {
            // Process X11 event
            process_x11_event();
        }
        
        // Check for expired timers
        let expired_timers = timer_manager.get_expired_timers();
        if !expired_timers.is_empty() {
            window.mark_frame_needs_regeneration();
        }
        
        // Check threads (if thread timer expired)
        if should_check_threads {
            if !layout_window.threads.is_empty() {
                window.mark_frame_needs_regeneration();
            }
        }
    }
}

fn wait_for_x11_event_with_timeout(
    display: *mut Display,
    fd: RawFd,
    timeout: Duration,
) -> bool {
    use libc::{select, timeval, fd_set, FD_ZERO, FD_SET, FD_ISSET};
    
    unsafe {
        // Check if events already queued
        if XPending(display) > 0 {
            return true;
        }
        
        let mut readfds: fd_set = std::mem::zeroed();
        FD_ZERO(&mut readfds);
        FD_SET(fd, &mut readfds);
        
        let mut tv = timeval {
            tv_sec: timeout.as_secs() as i64,
            tv_usec: timeout.subsec_micros() as i64,
        };
        
        let result = select(
            fd + 1,
            &mut readfds,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut tv,
        );
        
        result > 0 && FD_ISSET(fd, &readfds) != 0
    }
}
```

### Wayland Implementation (timerfd-based)

```rust
// In dll/src/desktop/shell2/linux/wayland/mod.rs

struct WaylandTimerManager {
    timers: HashMap<usize, (Timer, RawFd)>, // (timer, timerfd)
}

impl WaylandTimerManager {
    fn create_timer(&mut self, timer_id: usize, timer: &Timer) -> Result<()> {
        use libc::{timerfd_create, timerfd_settime, itimerspec, timespec};
        use libc::{CLOCK_MONOTONIC, TFD_NONBLOCK};
        
        unsafe {
            let fd = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK);
            if fd < 0 {
                return Err("Failed to create timerfd");
            }
            
            let interval_ms = timer.tick_millis();
            let interval_ns = interval_ms * 1_000_000;
            
            let spec = itimerspec {
                it_interval: timespec {
                    tv_sec: (interval_ns / 1_000_000_000) as i64,
                    tv_nsec: (interval_ns % 1_000_000_000) as i64,
                },
                it_value: timespec {
                    tv_sec: (interval_ns / 1_000_000_000) as i64,
                    tv_nsec: (interval_ns % 1_000_000_000) as i64,
                },
            };
            
            timerfd_settime(fd, 0, &spec, ptr::null_mut());
            
            self.timers.insert(timer_id, (timer.clone(), fd));
            Ok(())
        }
    }
}

impl PlatformWindowV2 for WaylandWindowV2 {
    fn start_timer(&mut self, timer_id: usize, timer: Timer) {
        self.timer_manager.create_timer(timer_id, &timer);
        
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.insert(
                azul_core::callbacks::TimerId { id: timer_id },
                timer
            );
        }
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some((_, fd)) = self.timer_manager.timers.remove(&timer_id) {
            unsafe {
                libc::close(fd);
            }
        }
        
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.timers.remove(&azul_core::callbacks::TimerId { id: timer_id });
        }
    }

    // ... thread management similar to X11 ...
}

// Update event loop to poll timerfds alongside Wayland events
```

---

## Testing Strategy

### Test 1: Single Timer
```rust
fn test_single_timer() {
    let callback = |_info: CallbackInfo| {
        println!("Timer fired!");
        CallCallbacksResult::default()
    };
    
    let timer = Timer {
        delay: Duration::from_millis(1000),
        interval: Some(Duration::from_millis(1000)),
        callback: callback.into(),
        node_id: None,
    };
    
    // Callback should create timer
    let result = CallCallbacksResult {
        timers: Some([(TimerId { id: 1 }, timer)].into()),
        ..Default::default()
    };
    
    // ✅ Verify SetTimer/NSTimer/timerfd called
    // ✅ Verify timer fires after 1000ms
    // ✅ Verify callback invoked
}
```

### Test 2: Thread Polling
```rust
fn test_thread_polling() {
    let callback = |_info: CallbackInfo| {
        let thread = Thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            RefAny::new(42)
        });
        
        CallCallbacksResult {
            threads: Some([(ThreadId::new(), thread)].into()),
            ..Default::default()
        }
    };
    
    // ✅ Verify thread polling timer starts (16ms)
    // ✅ Verify thread completion detected
    // ✅ Verify polling timer stops when threads empty
}
```

### Test 3: Timer Removal
```rust
fn test_timer_removal() {
    // Create timer
    let result1 = CallCallbacksResult {
        timers: Some([(TimerId { id: 1 }, timer)].into()),
        ..Default::default()
    };
    
    // Remove timer
    let result2 = CallCallbacksResult {
        timers_removed: Some([TimerId { id: 1 }].into()),
        ..Default::default()
    };
    
    // ✅ Verify KillTimer/invalidate/close called
    // ✅ Verify timer no longer fires
}
```

---

## Implementation Checklist

- [ ] **Phase 1: Trait Extension**
  - [ ] Add timer/thread methods to `PlatformWindowV2` trait
  - [ ] Update `process_callback_result_v2()` default implementation
  - [ ] Compile check (all platforms should fail with unimplemented methods)

- [ ] **Phase 2: Windows Implementation**
  - [ ] Implement `start_timer()` using `SetTimer()`
  - [ ] Implement `stop_timer()` using `KillTimer()`
  - [ ] Implement `start_thread_poll_timer()` with ID 0xFFFF
  - [ ] Implement `stop_thread_poll_timer()`
  - [ ] Implement `add_threads()` and `remove_threads()`
  - [ ] Update `WM_TIMER` handler to process expired timers
  - [ ] Update `WM_COMMAND` handler to call `process_callback_result_v2()`
  - [ ] Test on Windows

- [ ] **Phase 3: macOS Implementation**
  - [ ] Implement `start_timer()` using `NSTimer`
  - [ ] Implement `stop_timer()` using `[timer invalidate]`
  - [ ] Implement thread polling timer
  - [ ] Add `userTimerFired:` selector
  - [ ] Add `threadTimerFired:` selector
  - [ ] Update menu callback handler
  - [ ] Test on macOS

- [ ] **Phase 4: X11 Implementation**
  - [ ] Create `X11TimerManager` struct
  - [ ] Implement select-based event loop with timeout
  - [ ] Implement timer methods
  - [ ] Implement thread polling
  - [ ] Update event loop to check timers
  - [ ] Complete TODO in `process_callback_result_v2()`
  - [ ] Test on Linux X11

- [ ] **Phase 5: Wayland Implementation**
  - [ ] Create `WaylandTimerManager` struct
  - [ ] Implement timerfd-based timers
  - [ ] Add callback result processing (currently missing)
  - [ ] Implement thread polling
  - [ ] Update event loop to poll timerfds
  - [ ] Test on Linux Wayland

- [ ] **Phase 6: Integration Testing**
  - [ ] Test single timer on all platforms
  - [ ] Test multiple timers on all platforms
  - [ ] Test timer removal on all platforms
  - [ ] Test thread spawning on all platforms
  - [ ] Test thread completion detection on all platforms
  - [ ] Test thread polling timer lifecycle on all platforms

- [ ] **Phase 7: Documentation**
  - [ ] Update REFACTORING docs with timer/thread implementation
  - [ ] Add examples of timer/thread usage in callbacks
  - [ ] Document platform-specific timer behavior

---

## Summary

**Root Cause**: `CallCallbacksResult` timer/thread fields were never processed on any platform.

**Solution**: Extend `PlatformWindowV2` trait with platform-specific timer/thread methods, implement default processing in `process_callback_result_v2()`.

**Implementation Complexity**:
- **Windows**: ✅ Simple (SetTimer/KillTimer already used)
- **macOS**: ✅ Medium (NSTimer already implemented, just needs integration)
- **X11**: ⚠️ Complex (requires event loop refactoring with select/poll)
- **Wayland**: ⚠️ Complex (requires timerfd + event loop integration)

**Timeline Estimate**:
- Phase 1-2 (Trait + Windows): ~2-3 hours
- Phase 3 (macOS): ~2 hours
- Phase 4 (X11): ~4-6 hours (event loop refactoring)
- Phase 5 (Wayland): ~4-6 hours (timerfd + callback processing)
- Phase 6 (Testing): ~2-3 hours
- **Total**: ~14-20 hours

**Priority**: **HIGH** - This is a critical architectural bug affecting all callback-based timer/thread functionality.
