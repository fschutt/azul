//! Main event loop implementation for shell2
//!
//! This module provides the cross-platform run() function that starts
//! the application and event loop for each platform.

use std::{ffi::c_void, sync::Arc};

use azul_core::{refany::RefAny, resources::AppConfig};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use super::common::debug_server;
use super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace};

#[cfg(target_os = "macos")]
use super::macos::MacOSWindow;
use super::{PlatformWindow, WindowError};

/// Run the application with the given root window configuration
///
/// This function:
/// 1. Creates the root window using the platform-specific implementation
/// 2. Shows the window
/// 3. Enters the main event loop
/// 4. Processes events until the window is closed
///
/// # Platform-specific behavior
///
/// - **macOS**: Uses NSApplication.run() which blocks until app terminates, OR uses a manual event
///   loop if config.termination_behavior == ReturnToMain
/// - **Windows**: Manual event loop with GetMessage/TranslateMessage/DispatchMessage
/// - **Linux**: X11/Wayland event loop with appropriate polling
///
/// # Termination behavior
///
/// The behavior when all windows are closed is controlled by `config.termination_behavior`:
/// - `ReturnToMain`: Returns control to main() (if platform supports it)
/// - `RunForever`: Keeps app running until explicitly quit (macOS standard behavior)
/// - `EndProcess`: Calls std::process::exit(0) when last window closes (default)
#[cfg(target_os = "macos")]
pub fn run(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    use azul_core::resources::AppTerminationBehavior;
    use objc2::{rc::autoreleasepool, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask};
    use super::common::debug_server;

    // Note: Debug server is already started in App::create()
    
    debug_server::log(
        debug_server::LogLevel::Info,
        debug_server::LogCategory::EventLoop,
        "Starting macOS event loop setup",
        None,
    );

    autoreleasepool(|_| {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| WindowError::PlatformError("Not on main thread".into()))?;

        debug_server::log(debug_server::LogLevel::Debug, debug_server::LogCategory::EventLoop,
            "Got MainThreadMarker", None);

        // Create the root window with fc_cache and app_data
        // The window is automatically made visible after the first frame is ready
        debug_server::log(debug_server::LogLevel::Info, debug_server::LogCategory::Window,
            "Creating MacOSWindow...", None);
        let window =
            MacOSWindow::new_with_fc_cache(root_window, app_data.clone(), fc_cache.clone(), mtm)?;
        debug_server::log(debug_server::LogLevel::Info, debug_server::LogCategory::Window,
            "MacOSWindow created successfully", None);

        // Box and leak the window to get a stable pointer for the registry
        // SAFETY: We manage the lifetime through the registry
        let window_ptr = Box::into_raw(Box::new(window));
        let ns_window = unsafe { (*window_ptr).get_ns_window_ptr() };

        // CRITICAL: Set up back-pointers to the window
        // These enable the view and delegate to call back into MacOSWindow
        // SAFETY: window lives in the registry for the entire duration
        unsafe {
            (*window_ptr).setup_gl_view_back_pointer();
            (*window_ptr).finalize_delegate_pointer();
        }

        // Register window in global registry for multi-window support
        unsafe {
            super::macos::registry::register_window(ns_window, window_ptr);
        }

        // Request the first drawRect: call to display the pre-rendered frame
        unsafe {
            (*window_ptr).request_redraw();
        }

        // Get NSApplication and configure it
        let app = NSApplication::sharedApplication(mtm);
        unsafe {
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }

        // Choose event loop based on termination behavior
        match config.termination_behavior {
            AppTerminationBehavior::RunForever => {
                // Standard macOS behavior: Use NSApplication.run()
                // This blocks until the app is explicitly terminated (Cmd+Q or quit menu)
                debug_server::log(debug_server::LogLevel::Info, debug_server::LogCategory::EventLoop,
                    "Using NSApplication.run() - app will stay in dock when windows close", None);
                unsafe {
                    app.run();
                }
            }
            AppTerminationBehavior::ReturnToMain | AppTerminationBehavior::EndProcess => {
                // Manual event loop with multi-window support
                // Checks if all windows are closed and takes appropriate action
                let action = if config.termination_behavior == AppTerminationBehavior::ReturnToMain
                {
                    debug_server::log(debug_server::LogLevel::Info, debug_server::LogCategory::EventLoop,
                        "Using manual event loop - will return to main() when all windows close", None);
                    "return to main()"
                } else {
                    debug_server::log(debug_server::LogLevel::Info, debug_server::LogCategory::EventLoop,
                        "Using manual event loop - will exit process when all windows close", None);
                    "exit process"
                };

                loop {
                    autoreleasepool(|_| {

                        // PHASE 1: Process all pending native events (non-blocking)
                        loop {
                            let event = unsafe {
                                app.nextEventMatchingMask_untilDate_inMode_dequeue(
                                    NSEventMask::Any,
                                    None, // Don't wait - process immediately
                                    objc2_foundation::ns_string!("kCFRunLoopDefaultMode"),
                                    true,
                                )
                            };

                            if let Some(event) = event {
                                unsafe {
                                    app.sendEvent(&event);
                                }
                            } else {
                                // No more events to process
                                break;
                            }
                        }

                        // PHASE 2: Check if all windows are closed
                        if super::macos::registry::is_empty() {
                            match config.termination_behavior {
                                AppTerminationBehavior::ReturnToMain => {
                                    log_info!(debug_server::LogCategory::EventLoop, "[macOS] All windows closed, returning to main()");
                                    return;
                                }
                                AppTerminationBehavior::EndProcess => {
                                    log_info!(debug_server::LogCategory::EventLoop, "[macOS] All windows closed, terminating process");
                                    std::process::exit(0);
                                }
                                AppTerminationBehavior::RunForever => unreachable!(),
                            }
                        }

                        // PHASE 3: Process V2 state diffing and rendering for all windows
                        // (Optional - most V2 processing already happens in event handlers)
                        // This is where we process pending window creates for popup menus
                        let window_ptrs = super::macos::registry::get_all_window_ptrs();
                        for wptr in window_ptrs {
                            unsafe {
                                let window = &mut *wptr;

                                // Process pending window creates (for popup menus, dialogs, etc.)
                                while let Some(pending_create) = window.pending_window_creates.pop()
                                {
                                    log_debug!(debug_server::LogCategory::Window, "[macOS] Creating new window from queue (type: {:?})", pending_create.window_state.flags.window_type);

                                    match MacOSWindow::new_with_fc_cache(
                                        pending_create,
                                        app_data.clone(),
                                        fc_cache.clone(),
                                        mtm,
                                    ) {
                                        Ok(new_window) => {
                                            // Box and leak for stable pointer
                                            let new_window_ptr =
                                                Box::into_raw(Box::new(new_window));
                                            let new_ns_window =
                                                (*new_window_ptr).get_ns_window_ptr();

                                            // Setup back-pointers for new window
                                            (*new_window_ptr).setup_gl_view_back_pointer();
                                            (*new_window_ptr).finalize_delegate_pointer();

                                            // Register in global registry
                                            super::macos::registry::register_window(
                                                new_ns_window,
                                                new_window_ptr,
                                            );

                                            // Request initial redraw
                                            (*new_window_ptr).request_redraw();

                                            log_debug!(debug_server::LogCategory::Window, "[macOS] Successfully created and registered new window");
                                        }
                                        Err(e) => {
                                            log_error!(debug_server::LogCategory::Window, "[macOS] Failed to create window: {:?}", e);
                                        }
                                    }
                                }
                            }
                        }

                        // PHASE 4: Wait for next event (blocking)
                        // This prevents busy-waiting and reduces CPU usage to near-zero when idle
                        let distant_future = objc2_foundation::NSDate::distantFuture();
                        let event = unsafe {
                            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                                NSEventMask::Any,
                                Some(&distant_future), // Block until event arrives
                                objc2_foundation::ns_string!("kCFRunLoopDefaultMode"),
                                true,
                            )
                        };

                        if let Some(event) = event {
                            unsafe {
                                app.sendEvent(&event);
                            }
                        }
                    });
                }
            }
        }

        Ok(())
    })
}

// Store initial options globally for the AppDelegate to retrieve.
// Unsafe, but simple for this minimal example.
#[cfg(target_os = "ios")]
pub(super) static mut INITIAL_OPTIONS: Option<(AppConfig, Arc<FcFontCache>, WindowCreateOptions)> =
    None;

// On iOS, the `run` function doesn't manage an event loop.
// Instead, it calls a bootstrap function that sets up the native
// UIKit application and hands control over to the OS. This call never returns.
#[cfg(target_os = "ios")]
pub fn run(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    unsafe {
        INITIAL_OPTIONS = Some((app_data, config, fc_cache, root_window));
        crate::desktop::shell2::ios::launch_app();
        Ok(()) // Unreachable
    }
}

#[cfg(target_os = "windows")]
pub fn run(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    log_trace!(LogCategory::Window, "[shell2::run] Windows run() called");
    use std::cell::RefCell;

    use azul_core::resources::AppTerminationBehavior;

    use super::windows::{dlopen::MSG, registry, Win32Window};

    log_trace!(LogCategory::Window, "[shell2::run] imports done");
    // Wrap app_data in Arc<RefCell<>> for shared access
    log_trace!(LogCategory::Window, "[shell2::run] wrapping app_data in Arc<RefCell<>>");
    let app_data_arc = Arc::new(RefCell::new(app_data));
    log_trace!(LogCategory::Window, "[shell2::run] app_data wrapped");

    // Create the root window
    log_trace!(LogCategory::Window, "[shell2::run] calling Win32Window::new");
    let window = Win32Window::new(root_window, fc_cache.clone(), app_data_arc.clone())?;
    log_trace!(LogCategory::Window, "[shell2::run] Win32Window::new returned successfully");

    // Store the window pointer in the user data field for the window procedure
    // and register in global registry for multi-window support
    // SAFETY: We are boxing the window and then leaking it. This is necessary
    // so that the pointer remains valid for the lifetime of the window.
    log_trace!(LogCategory::Window, "[shell2::run] boxing window");
    let window_ptr = Box::into_raw(Box::new(window));
    log_trace!(LogCategory::Window, "[shell2::run] getting hwnd");
    let hwnd = unsafe { (*window_ptr).hwnd };
    log_trace!(LogCategory::Window, "[shell2::run] got hwnd: {:?}", hwnd);

    unsafe {
        use super::windows::dlopen::constants::GWLP_USERDATA;
        log_trace!(LogCategory::Window, "[shell2::run] calling SetWindowLongPtrW");
        ((*window_ptr).win32.user32.SetWindowLongPtrW)(hwnd, GWLP_USERDATA, window_ptr as isize);
        log_trace!(LogCategory::Window, "[shell2::run] SetWindowLongPtrW done");

        // Register in global window registry
        log_trace!(LogCategory::Window, "[shell2::run] registering window in global registry");
        registry::register_window(hwnd, window_ptr);
        log_trace!(LogCategory::Window, "[shell2::run] window registered");
        
        // NOTE: Window is NOT shown here! It will be shown automatically by
        // render_and_present() after the first SwapBuffers completes.
        // This ensures the window appears with content, not a black/white flash.
        log_trace!(LogCategory::Window, "[shell2::run] window will be shown after first frame renders");
    }

    log_trace!(LogCategory::Window, "[shell2::run] entering main event loop");
    // Main event loop with multi-window support and V2 state diffing
    // Uses WaitMessage() to block efficiently when idle (no busy-waiting)
    // Architecture:
    // 1. Process all pending native events (updates current_window_state)
    // 2. V2 state diff + callback dispatch (compares previous vs current)
    // 3. Render all windows that need updates
    // 4. Block until next event (zero CPU when idle)
    loop {
        // Get all active window handles from registry
        let window_handles = registry::get_all_window_handles();

        if window_handles.is_empty() {
            // All windows closed
            break;
        }

        // PHASE 1: Process all pending native events (non-blocking)
        // This updates current_window_state for each window
        let mut had_messages = false;

        for hwnd in &window_handles {
            if let Some(wptr) = registry::get_window(*hwnd) {
                unsafe {
                    let window = &mut *wptr;
                    let mut msg: MSG = std::mem::zeroed();

                    // PeekMessage with PM_REMOVE to process all pending messages for this window
                    while (window.win32.user32.PeekMessageW)(
                        &mut msg, *hwnd, 0, 0, 1, // PM_REMOVE
                    ) > 0
                    {
                        had_messages = true;

                        // Check for WM_QUIT
                        if msg.message == 0x0012 {
                            // WM_QUIT - exit event loop
                            return Ok(());
                        }

                        (window.win32.user32.TranslateMessage)(&msg);
                        (window.win32.user32.DispatchMessageW)(&msg);
                    }
                }
            }
        }

        // PHASE 2: V2 state diffing and callback dispatch
        // This is where callbacks fire (comparing previous_window_state vs current_window_state)
        // NOTE: window_proc already calls process_window_events_recursive_v2() for mouse/keyboard
        // events, but this catches any additional state changes and processes pending window
        // creates
        for hwnd in &window_handles {
            if let Some(window_ptr_from_registry) = registry::get_window(*hwnd) {
                unsafe {
                    let window = &mut *window_ptr_from_registry;

                    // Save previous state if not already done
                    if window.previous_window_state.is_none() {
                        window.previous_window_state = Some(window.current_window_state.clone());
                    }

                    // Process pending window creates (for popup menus, dialogs, etc.)
                    while let Some(pending_create) = window.pending_window_creates.pop() {
                        log_debug!(debug_server::LogCategory::Window, "[Windows] Creating new window from queue (type: {:?})", pending_create.window_state.flags.window_type);

                        match Win32Window::new(
                            pending_create,
                            window.fc_cache.clone(),
                            window.app_data.clone(),
                        ) {
                            Ok(new_window) => {
                                // Box and leak for stable pointer
                                let new_window_ptr = Box::into_raw(Box::new(new_window));
                                let new_hwnd = unsafe { (*new_window_ptr).hwnd };

                                // Set window user data for window_proc
                                use super::windows::dlopen::constants::GWLP_USERDATA;
                                ((*new_window_ptr).win32.user32.SetWindowLongPtrW)(
                                    new_hwnd,
                                    GWLP_USERDATA,
                                    new_window_ptr as isize,
                                );

                                // Register in global registry
                                registry::register_window(new_hwnd, new_window_ptr);

                                log_debug!(debug_server::LogCategory::Window, "[Windows] Successfully created and registered new window");
                            }
                            Err(e) => {
                                log_error!(debug_server::LogCategory::Window, "[Windows] Failed to create window: {:?}", e);
                            }
                        }
                    }
                }
            }
        }

        // PHASE 3: Render all windows that need updates
        for hwnd in &window_handles {
            if let Some(window_ptr_from_registry) = registry::get_window(*hwnd) {
                unsafe {
                    let window = &mut *window_ptr_from_registry;

                    if window.frame_needs_regeneration {
                        if let Err(e) = window.regenerate_layout() {
                            log_error!(debug_server::LogCategory::Layout, "[Windows] Layout regeneration error: {}", e);
                        }
                        window.frame_needs_regeneration = false;

                        // Request WM_PAINT
                        use std::ptr;
                        (window.win32.user32.InvalidateRect)(*hwnd, ptr::null(), 0);
                    }
                }
            }
        }

        // PHASE 4: Wait for next event (blocks until event available - zero CPU when idle)
        // This replaces the old sleep(1ms) with proper blocking
        // WaitMessage() waits for ANY message in the thread's queue (all windows share the same
        // thread)
        if !had_messages {
            unsafe {
                // Get any window to access Win32 libraries
                if let Some(first_hwnd) = window_handles.first() {
                    if let Some(wptr) = registry::get_window(*first_hwnd) {
                        let window = &*wptr;
                        // WaitMessage() blocks until ANY message is available in the thread's
                        // message queue It doesn't matter which window we
                        // call it from - all windows share the same thread
                        (window.win32.user32.WaitMessage)();
                    }
                }
            }
        }
    }

    // Clean up: Unregister and drop all windows
    let window_handles = registry::get_all_window_handles();
    for hwnd in window_handles {
        if let Some(win_ptr) = registry::unregister_window(hwnd) {
            // SAFETY: We created this pointer with Box::into_raw
            unsafe {
                drop(Box::from_raw(win_ptr));
            }
        }
    }

    // Handle termination behavior
    match config.termination_behavior {
        AppTerminationBehavior::EndProcess => {
            std::process::exit(0);
        }
        AppTerminationBehavior::ReturnToMain => {
            // Return normally to allow cleanup
        }
        AppTerminationBehavior::RunForever => {
            // Should not exit - but all windows are closed, so return
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn run(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    use std::cell::RefCell;

    use azul_core::resources::AppTerminationBehavior;

    use super::linux::{registry, AppResources, LinuxWindow};

    // Initialize shared resources once at startup
    let resources = Arc::new(AppResources::new(config.clone(), fc_cache));

    log_debug!(debug_server::LogCategory::EventLoop, "[Linux] Creating root window with shared resources");

    // Wrap app_data in Arc<RefCell<>> for shared access
    let app_data_arc = Arc::new(RefCell::new(app_data));

    // Create the root window
    let window = LinuxWindow::new_with_resources(root_window, app_data_arc, resources.clone())?;

    // Box and register window in global registry
    let window_ptr = Box::into_raw(Box::new(window));

    // Get window ID and display for registration
    let (window_id, display_ptr) = unsafe {
        match &*window_ptr {
            LinuxWindow::X11(x11_window) => (x11_window.window, x11_window.display),
            LinuxWindow::Wayland(wayland_window) => {
                // For Wayland, we use the wl_display pointer as the window ID
                // This is safe because display pointers are unique per window
                (
                    wayland_window.display as u64,
                    wayland_window.display as *mut c_void,
                )
            }
        }
    };

    // Register the window
    unsafe {
        registry::register_x11_window(window_id, window_ptr as *mut _);
    }

    log_debug!(debug_server::LogCategory::EventLoop, "[Linux] Window registered (ID: {}), entering event loop", window_id);

    // Main event loop with multi-window support
    loop {
        // Get all active window IDs
        let window_ids = registry::get_all_x11_window_ids();

        if window_ids.is_empty() {
            log_info!(debug_server::LogCategory::EventLoop, "[Linux] All windows closed, exiting event loop");
            break;
        }

        let is_multi_window = window_ids.len() > 1;

        // Process events for all windows
        for wid in &window_ids {
            if let Some(win_ptr) = unsafe { registry::get_x11_window(*wid) } {
                let window = unsafe { &mut *(win_ptr as *mut LinuxWindow) };

                // Poll all pending events (non-blocking)
                while window.poll_event().is_some() {
                    // Event handling is done inside poll_event
                }
            }
        }

        // PHASE 3: Process pending window creates for all windows
        // This processes the queue populated by callbacks (context menus, dialogs, etc.)
        for wid in &window_ids {
            if let Some(win_ptr) = unsafe { registry::get_x11_window(*wid) } {
                let window = unsafe { &mut *(win_ptr as *mut LinuxWindow) };

                match window {
                    LinuxWindow::X11(x11_window) => {
                        while let Some(pending_create) = x11_window.pending_window_creates.pop() {
                            log_debug!(debug_server::LogCategory::Window, "[Linux] Creating new X11 window from queue (type: {:?})", pending_create.window_state.flags.window_type);

                            match super::linux::x11::X11Window::new_with_resources(
                                pending_create,
                                x11_window.resources.clone(),
                            ) {
                                Ok(new_window) => {
                                    let new_x11_window = LinuxWindow::X11(new_window);
                                    let new_window_ptr = Box::into_raw(Box::new(new_x11_window));

                                    // Get the X11 window ID for registration
                                    let new_window_id = unsafe {
                                        if let LinuxWindow::X11(ref w) = *new_window_ptr {
                                            w.window
                                        } else {
                                            unreachable!()
                                        }
                                    };

                                    // Register in global registry
                                    unsafe {
                                        registry::register_x11_window(
                                            new_window_id,
                                            new_window_ptr as *mut _,
                                        );
                                    }

                                    log_debug!(debug_server::LogCategory::Window, "[Linux] Successfully created and registered new X11 window (ID: {})", new_window_id);

                                    // Request initial redraw
                                    unsafe {
                                        if let LinuxWindow::X11(ref mut w) = *new_window_ptr {
                                            w.request_redraw();
                                        }
                                    }
                                }
                                Err(e) => {
                                    log_error!(debug_server::LogCategory::Window, "[Linux] Failed to create X11 window: {:?}", e);
                                }
                            }
                        }
                    }
                    LinuxWindow::Wayland(wayland_window) => {
                        while let Some(pending_create) = wayland_window.pending_window_creates.pop()
                        {
                            log_debug!(debug_server::LogCategory::Window, "[Linux] Creating new Wayland window from queue (type: {:?})", pending_create.window_state.flags.window_type);

                            match super::linux::wayland::WaylandWindow::new(
                                pending_create,
                                wayland_window.resources.clone(),
                            ) {
                                Ok(new_window) => {
                                    let new_wayland_window = LinuxWindow::Wayland(new_window);
                                    let new_window_ptr =
                                        Box::into_raw(Box::new(new_wayland_window));

                                    // Get the Wayland display pointer for registration
                                    let new_window_id = unsafe {
                                        if let LinuxWindow::Wayland(ref w) = *new_window_ptr {
                                            w.display as u64
                                        } else {
                                            unreachable!()
                                        }
                                    };

                                    // Register in global registry
                                    unsafe {
                                        registry::register_x11_window(
                                            new_window_id,
                                            new_window_ptr as *mut _,
                                        );
                                    }

                                    log_debug!(debug_server::LogCategory::Window, "[Linux] Successfully created and registered new Wayland window (ID: {})", new_window_id);

                                    // Request initial redraw
                                    unsafe {
                                        if let LinuxWindow::Wayland(ref mut w) = *new_window_ptr {
                                            w.request_redraw();
                                        }
                                    }
                                }
                                Err(e) => {
                                    log_error!(debug_server::LogCategory::Window, "[Linux] Failed to create Wayland window: {:?}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Wait strategy based on number of windows
        if !is_multi_window {
            // Single window: Block on XNextEvent (efficient)
            if let Some(win_ptr) = unsafe { registry::get_x11_window(window_ids[0]) } {
                let window = unsafe { &mut *(win_ptr as *mut LinuxWindow) };
                window.wait_for_events()?;
            }
        } else {
            // Multi-window: Use select() on X11 connection fd to wait efficiently
            // This is much better than sleep() as it wakes immediately when events arrive
            wait_for_x11_connection_activity(display_ptr)?;
        }
    }

    // Clean up: Unregister and drop all windows
    log_debug!(debug_server::LogCategory::EventLoop, "[Linux] Cleaning up windows");
    let window_ids = registry::get_all_x11_window_ids();
    for wid in window_ids {
        if let Some(win_ptr) = registry::unregister_x11_window(wid) {
            unsafe {
                drop(Box::from_raw(win_ptr as *mut LinuxWindow));
            }
        }
    }

    // Handle termination behavior
    match config.termination_behavior {
        AppTerminationBehavior::EndProcess => {
            log_info!(debug_server::LogCategory::EventLoop, "[Linux] Terminating process");
            std::process::exit(0);
        }
        AppTerminationBehavior::ReturnToMain => {
            log_info!(debug_server::LogCategory::EventLoop, "[Linux] Returning to main()");
            // Return normally
        }
        AppTerminationBehavior::RunForever => {
            log_debug!(debug_server::LogCategory::EventLoop, "[Linux] RunForever mode - but all windows closed");
            // Should not exit, but all windows are closed
        }
    }

    Ok(())
}

/// Wait for activity on the X11 connection using select() with timeout
///
/// This is more efficient than sleeping as it wakes immediately when events arrive.
/// Uses a 16ms timeout to ensure timers fire even without window events.
#[cfg(target_os = "linux")]
fn wait_for_x11_connection_activity(display: *mut std::ffi::c_void) -> Result<(), WindowError> {
    use std::mem;

    use super::linux::x11::{defines::Display, dlopen::Xlib};

    // Get the X11 library to access XConnectionNumber
    let xlib = Xlib::new()
        .map_err(|e| WindowError::PlatformError(format!("Failed to load Xlib: {:?}", e)))?;

    // Get the file descriptor for the X11 connection
    let connection_fd = unsafe { (xlib.XConnectionNumber)(display as *mut Display) };

    // Use select() to wait for events on the X11 connection
    unsafe {
        let mut read_fds: libc::fd_set = mem::zeroed();
        libc::FD_ZERO(&mut read_fds);
        libc::FD_SET(connection_fd, &mut read_fds);

        // Use 16ms timeout to ensure timers fire even without window events
        // This allows ~60 timer checks per second while still being efficient
        let mut timeout = libc::timeval {
            tv_sec: 0,
            tv_usec: 16_000, // 16ms = 16000 microseconds
        };

        let result = libc::select(
            connection_fd + 1,
            &mut read_fds,
            std::ptr::null_mut(), // No write fds
            std::ptr::null_mut(), // No error fds
            &mut timeout,         // 16ms timeout for timer polling
        );

        if result < 0 {
            let errno = *libc::__errno_location();
            // EINTR is okay - just means a signal interrupted us
            if errno != libc::EINTR {
                return Err(WindowError::PlatformError(
                    format!("select() failed while waiting for X11 events: errno={}", errno),
                ));
            }
        }
        // result == 0 means timeout - that's fine, we'll check timers
        // result > 0 means events are ready
    }

    Ok(())
}
