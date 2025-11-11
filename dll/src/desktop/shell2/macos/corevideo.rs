//! CoreVideo FFI bindings via dlopen for backward compatibility
//!
//! This module provides safe Rust wrappers around CoreVideo C APIs,
//! specifically CVDisplayLink for proper VSYNC synchronization.
//!
//! We use dlopen instead of static linking to support older macOS versions
//! where CoreVideo might not be available or behave differently.

use std::{
    os::raw::{c_int, c_void},
    sync::Arc,
};

/// CVDisplayLink opaque pointer
pub type CVDisplayLinkRef = *mut c_void;

/// CVReturn type (result code)
pub type CVReturn = i32;

/// Success return code
pub const K_CV_RETURN_SUCCESS: CVReturn = 0;

/// CVTimeStamp structure (simplified - we only need the minimal fields)
#[repr(C)]
pub struct CVTimeStamp {
    pub version: u32,
    pub video_time_scale: i32,
    pub video_time: i64,
    pub host_time: u64,
    pub rate_scalar: f64,
    pub video_refresh_period: i64,
    pub smpte_time: CVSMPTETime,
    pub flags: u64,
    pub reserved: u64,
}

#[repr(C)]
pub struct CVSMPTETime {
    pub subframes: i16,
    pub subframe_divisor: i16,
    pub counter: u32,
    pub type_: u32,
    pub flags: u32,
    pub hours: i16,
    pub minutes: i16,
    pub seconds: i16,
    pub frames: i16,
}

/// Display link output callback
pub type CVDisplayLinkOutputCallback = extern "C" fn(
    display_link: CVDisplayLinkRef,
    in_now: *const CVTimeStamp,
    in_output_time: *const CVTimeStamp,
    flags_in: u64,
    flags_out: *mut u64,
    display_link_context: *mut c_void,
) -> CVReturn;

/// CoreVideo function pointers loaded via dlopen
pub struct CoreVideoFunctions {
    // CVDisplayLink functions
    // Note: CVDisplayLinkCreateWithCGDisplays takes an array of display IDs and count
    cv_display_link_create_with_cg_displays: unsafe extern "C" fn(
        display_array: *const u32,
        count: u32,
        display_link_out: *mut CVDisplayLinkRef,
    ) -> CVReturn,
    cv_display_link_set_output_callback: unsafe extern "C" fn(
        display_link: CVDisplayLinkRef,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CVReturn,
    cv_display_link_start: unsafe extern "C" fn(display_link: CVDisplayLinkRef) -> CVReturn,
    cv_display_link_stop: unsafe extern "C" fn(display_link: CVDisplayLinkRef) -> CVReturn,
    cv_display_link_release: unsafe extern "C" fn(display_link: CVDisplayLinkRef),
    cv_display_link_is_running: unsafe extern "C" fn(display_link: CVDisplayLinkRef) -> bool,

    // Keep the library handle to prevent unloading
    #[allow(dead_code)]
    lib: libloading::Library,
}

impl CoreVideoFunctions {
    /// Load CoreVideo functions via dlopen
    ///
    /// Returns None if CoreVideo framework is not available (older macOS versions)
    pub fn load() -> Result<Arc<Self>, String> {
        unsafe {
            // Try to load CoreVideo framework
            let lib = libloading::Library::new(
                "/System/Library/Frameworks/CoreVideo.framework/CoreVideo",
            )
            .map_err(|e| format!("Failed to load CoreVideo framework: {}", e))?;

            // Load CVDisplayLink functions
            let cv_display_link_create_with_cg_displays = *lib
                .get(b"CVDisplayLinkCreateWithCGDisplays\0")
                .map_err(|e| format!("CVDisplayLinkCreateWithCGDisplays not found: {}", e))?;

            let cv_display_link_set_output_callback = *lib
                .get(b"CVDisplayLinkSetOutputCallback\0")
                .map_err(|e| format!("CVDisplayLinkSetOutputCallback not found: {}", e))?;

            let cv_display_link_start = *lib
                .get(b"CVDisplayLinkStart\0")
                .map_err(|e| format!("CVDisplayLinkStart not found: {}", e))?;

            let cv_display_link_stop = *lib
                .get(b"CVDisplayLinkStop\0")
                .map_err(|e| format!("CVDisplayLinkStop not found: {}", e))?;

            let cv_display_link_release = *lib
                .get(b"CVDisplayLinkRelease\0")
                .map_err(|e| format!("CVDisplayLinkRelease not found: {}", e))?;

            let cv_display_link_is_running = *lib
                .get(b"CVDisplayLinkIsRunning\0")
                .map_err(|e| format!("CVDisplayLinkIsRunning not found: {}", e))?;

            Ok(Arc::new(Self {
                cv_display_link_create_with_cg_displays,
                cv_display_link_set_output_callback,
                cv_display_link_start,
                cv_display_link_stop,
                cv_display_link_release,
                cv_display_link_is_running,
                lib,
            }))
        }
    }

    /// Create a CVDisplayLink for a specific display
    pub fn create_display_link(&self, display_id: u32) -> Result<CVDisplayLinkRef, CVReturn> {
        unsafe {
            let mut display_link: CVDisplayLinkRef = std::ptr::null_mut();
            let display_array = [display_id];

            let result = (self.cv_display_link_create_with_cg_displays)(
                display_array.as_ptr(),
                1, // count
                &mut display_link,
            );

            if result == K_CV_RETURN_SUCCESS {
                if display_link.is_null() {
                    return Err(-1);
                }
                Ok(display_link)
            } else {
                Err(result)
            }
        }
    }

    /// Set output callback for CVDisplayLink
    pub fn set_output_callback(
        &self,
        display_link: CVDisplayLinkRef,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CVReturn {
        unsafe { (self.cv_display_link_set_output_callback)(display_link, callback, user_info) }
    }

    /// Start the CVDisplayLink
    pub fn start(&self, display_link: CVDisplayLinkRef) -> CVReturn {
        unsafe { (self.cv_display_link_start)(display_link) }
    }

    /// Stop the CVDisplayLink
    pub fn stop(&self, display_link: CVDisplayLinkRef) -> CVReturn {
        unsafe { (self.cv_display_link_stop)(display_link) }
    }

    /// Release the CVDisplayLink
    pub fn release(&self, display_link: CVDisplayLinkRef) {
        unsafe { (self.cv_display_link_release)(display_link) }
    }

    /// Check if the CVDisplayLink is running
    pub fn is_running(&self, display_link: CVDisplayLinkRef) -> bool {
        unsafe { (self.cv_display_link_is_running)(display_link) }
    }
}

/// RAII wrapper for CVDisplayLink
pub struct DisplayLink {
    display_link: CVDisplayLinkRef,
    cv_functions: Arc<CoreVideoFunctions>,
}

impl DisplayLink {
    /// Create a new DisplayLink for a specific display
    pub fn new(display_id: u32, cv_functions: Arc<CoreVideoFunctions>) -> Result<Self, CVReturn> {
        let display_link = cv_functions.create_display_link(display_id)?;
        Ok(Self {
            display_link,
            cv_functions,
        })
    }

    /// Set the output callback
    pub fn set_output_callback(
        &self,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CVReturn {
        self.cv_functions
            .set_output_callback(self.display_link, callback, user_info)
    }

    /// Start the display link
    pub fn start(&self) -> CVReturn {
        self.cv_functions.start(self.display_link)
    }

    /// Stop the display link
    pub fn stop(&self) -> CVReturn {
        self.cv_functions.stop(self.display_link)
    }

    /// Check if the display link is running
    pub fn is_running(&self) -> bool {
        self.cv_functions.is_running(self.display_link)
    }

    /// Get the raw CVDisplayLinkRef
    pub fn as_ptr(&self) -> CVDisplayLinkRef {
        self.display_link
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        if !self.display_link.is_null() {
            // Stop if running
            if self.is_running() {
                self.stop();
            }
            // Release
            self.cv_functions.release(self.display_link);
        }
    }
}

// Safety: CVDisplayLink is thread-safe according to Apple documentation
unsafe impl Send for DisplayLink {}
unsafe impl Sync for DisplayLink {}
