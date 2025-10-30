//! Main event loop implementation for shell2
//!
//! This module provides the cross-platform run() function that starts
//! the application and event loop for each platform.

use std::{ffi::c_void, sync::Arc};

use azul_core::resources::AppConfig;
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

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
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    use azul_core::resources::AppTerminationBehavior;
    use objc2::{rc::autoreleasepool, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask};

    autoreleasepool(|_| {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| WindowError::PlatformError("Not on main thread".into()))?;

        // Create the root window with fc_cache
        // The window is automatically made visible after the first frame is ready
        let mut window = MacOSWindow::new_with_fc_cache(root_window, fc_cache, mtm)?;

        // CRITICAL: Set up back-pointers to the window
        // These enable the view and delegate to call back into MacOSWindow
        // SAFETY: window lives for the entire duration of this function,
        // and the view/delegate are owned by the window
        unsafe {
            window.setup_gl_view_back_pointer();
            window.finalize_delegate_pointer();
        }

        // Request the first drawRect: call to display the pre-rendered frame
        window.request_redraw();

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
                eprintln!(
                    "[Event Loop] Using NSApplication.run() - app will stay in dock when windows \
                     close"
                );
                unsafe {
                    app.run();
                }
            }
            AppTerminationBehavior::ReturnToMain | AppTerminationBehavior::EndProcess => {
                // Manual event loop: Checks if windows are closed and takes appropriate action
                let action = if config.termination_behavior == AppTerminationBehavior::ReturnToMain
                {
                    eprintln!(
                        "[Event Loop] Using manual event loop - will return to main() when all \
                         windows close"
                    );
                    "return to main()"
                } else {
                    eprintln!(
                        "[Event Loop] Using manual event loop - will exit process when all \
                         windows close"
                    );
                    "exit process"
                };

                loop {
                    autoreleasepool(|_| {
                        // First, process all pending events (non-blocking)
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

                        // Check if window is still open
                        if !window.is_open() {
                            match config.termination_behavior {
                                AppTerminationBehavior::ReturnToMain => {
                                    eprintln!(
                                        "[Event Loop] All windows closed, returning to main()"
                                    );
                                    return;
                                }
                                AppTerminationBehavior::EndProcess => {
                                    eprintln!(
                                        "[Event Loop] All windows closed, terminating process"
                                    );
                                    std::process::exit(0);
                                }
                                AppTerminationBehavior::RunForever => unreachable!(),
                            }
                        }

                        // After processing all events, wait for next event (blocking)
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
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    unsafe {
        INITIAL_OPTIONS = Some((config, fc_cache, root_window));
        crate::desktop::shell2::ios::launch_app();
        Ok(()) // Unreachable
    }
}

#[cfg(target_os = "windows")]
pub fn run(
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    use std::cell::RefCell;

    use azul_core::{refany::RefAny, resources::AppTerminationBehavior};

    use super::windows::{dlopen::MSG, registry, Win32Window};

    // Create app_data (placeholder for now - should be passed from App)
    let app_data = Arc::new(RefCell::new(RefAny::new(())));

    // Create the root window
    let window = Win32Window::new(root_window, fc_cache.clone(), app_data.clone())?;

    // Store the window pointer in the user data field for the window procedure
    // and register in global registry for multi-window support
    // SAFETY: We are boxing the window and then leaking it. This is necessary
    // so that the pointer remains valid for the lifetime of the window.
    let window_ptr = Box::into_raw(Box::new(window));
    let hwnd = unsafe { (*window_ptr).hwnd };

    unsafe {
        use super::windows::dlopen::constants::GWLP_USERDATA;
        ((*window_ptr).win32.user32.SetWindowLongPtrW)(hwnd, GWLP_USERDATA, window_ptr as isize);

        // Register in global window registry
        registry::register_window(hwnd, window_ptr);
    }

    // Main event loop with multi-window support
    // For single-window apps, GetMessageW blocks until the next event
    // For multi-window apps, we use PeekMessageW + sleep(1ms) to avoid blocking
    loop {
        // Get all active window handles from registry
        let window_handles = registry::get_all_window_handles();

        if window_handles.is_empty() {
            // All windows closed
            break;
        }

        let is_multi_window = window_handles.len() > 1;

        if is_multi_window {
            // Multi-window: Use PeekMessage for all windows (non-blocking)
            let mut had_messages = false;

            for hwnd in &window_handles {
                unsafe {
                    let mut msg: MSG = std::mem::zeroed();

                    // Check if there's a message for this window
                    let has_msg = ((*window_ptr).win32.user32.PeekMessageW)(
                        &mut msg, *hwnd, 0, 0, 1, // PM_REMOVE
                    ) > 0;

                    if has_msg {
                        had_messages = true;
                        ((*window_ptr).win32.user32.TranslateMessage)(&msg);
                        ((*window_ptr).win32.user32.DispatchMessageW)(&msg);
                    }
                }
            }

            // If no messages for any window, sleep briefly to reduce CPU usage
            if !had_messages {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        } else {
            // Single-window: Use GetMessage (blocks until message arrives)
            let hwnd = window_handles[0];
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                let result = ((*window_ptr).win32.user32.GetMessageW)(&mut msg, hwnd, 0, 0);

                if result > 0 {
                    ((*window_ptr).win32.user32.TranslateMessage)(&msg);
                    ((*window_ptr).win32.user32.DispatchMessageW)(&msg);
                } else {
                    // WM_QUIT received or error
                    break;
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
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    use std::cell::RefCell;

    use azul_core::{refany::RefAny, resources::AppTerminationBehavior};

    use super::linux::{registry, AppResources, LinuxWindow};

    // Initialize shared resources once at startup
    let resources = Arc::new(AppResources::new(config.clone(), fc_cache));

    eprintln!("[Linux run()] Creating root window with shared resources");

    // Create the root window
    let window = LinuxWindow::new_with_resources(root_window, resources.clone())?;

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

    eprintln!(
        "[Linux run()] Window registered (ID: {}), entering event loop",
        window_id
    );

    // Main event loop with multi-window support
    loop {
        // Get all active window IDs
        let window_ids = registry::get_all_x11_window_ids();

        if window_ids.is_empty() {
            eprintln!("[Linux run()] All windows closed, exiting event loop");
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
    eprintln!("[Linux run()] Cleaning up windows");
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
            eprintln!("[Linux run()] Terminating process");
            std::process::exit(0);
        }
        AppTerminationBehavior::ReturnToMain => {
            eprintln!("[Linux run()] Returning to main()");
            // Return normally
        }
        AppTerminationBehavior::RunForever => {
            eprintln!("[Linux run()] RunForever mode - but all windows closed");
            // Should not exit, but all windows are closed
        }
    }

    Ok(())
}

/// Wait for activity on the X11 connection using select()
///
/// This is more efficient than sleeping as it wakes immediately when events arrive.
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

        // Wait indefinitely for events (no timeout)
        let result = libc::select(
            connection_fd + 1,
            &mut read_fds,
            std::ptr::null_mut(), // No write fds
            std::ptr::null_mut(), // No error fds
            std::ptr::null_mut(), // No timeout - block indefinitely
        );

        if result < 0 {
            return Err(WindowError::PlatformError(
                "select() failed while waiting for X11 events".into(),
            ));
        }
    }

    Ok(())
}
