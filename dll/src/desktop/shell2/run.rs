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
/// - **macOS**: Uses NSApplication.run() which blocks until app terminates
/// - **Windows**: Manual event loop with GetMessage/TranslateMessage/DispatchMessage
/// - **Linux**: X11/Wayland event loop with appropriate polling
#[cfg(target_os = "macos")]
pub fn run(
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    use objc2::{rc::autoreleasepool, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

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

        // Enter the main event loop
        // Note: NSApplication.run() blocks until the app terminates
        unsafe {
            app.run();
        }

        Ok(())
    })
}

#[cfg(target_os = "windows")]
pub fn run(
    _config: AppConfig,
    _fc_cache: Arc<FcFontCache>,
    _root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    // TODO: Implement Windows event loop
    Err(WindowError::PlatformError(
        "Windows shell2 not yet implemented".into(),
    ))
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
