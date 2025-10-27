//! Main event loop implementation for shell2
//!
//! This module provides the cross-platform run() function that starts
//! the application and event loop for each platform.

use std::sync::Arc;

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
                        // Process all pending events
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
                    });
                }
            }
        }

        Ok(())
    })
}

#[cfg(target_os = "windows")]
pub fn run(
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    use std::cell::RefCell;

    use azul_core::refany::RefAny;

    use super::windows::Win32Window;

    // Create app_data (placeholder for now - should be passed from App)
    let app_data = Arc::new(RefCell::new(RefAny::new(())));

    // Create the root window
    let mut window = Win32Window::new(root_window, fc_cache.clone(), app_data)?;

    // Windows event loop using GetMessage/DispatchMessage
    use super::windows::dlopen::{MSG, WPARAM};

    unsafe {
        let mut msg: MSG = MSG {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: super::windows::dlopen::POINT { x: 0, y: 0 },
        };

        // Main message loop
        while window.is_open {
            // Get message from queue (blocks until message arrives)
            let result = (window.win32.user32.GetMessageW)(&mut msg, window.hwnd, 0, 0);

            if result == 0 {
                // WM_QUIT received
                break;
            } else if result < 0 {
                // Error occurred
                return Err(WindowError::PlatformError("GetMessage failed".into()));
            }

            // Translate and dispatch message
            (window.win32.user32.TranslateMessage)(&msg);
            (window.win32.user32.DispatchMessageW)(&msg);
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn run(
    _config: AppConfig,
    _fc_cache: Arc<FcFontCache>,
    _root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    // TODO: Implement Linux event loop
    Err(WindowError::PlatformError(
        "Linux shell2 not yet implemented".into(),
    ))
}
