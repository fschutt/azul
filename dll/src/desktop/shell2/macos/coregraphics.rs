//! Core Graphics display-related FFI bindings
//!
//! This module provides access to CGDirectDisplayID and related
//! display enumeration functions for monitor identification.

use std::sync::Arc;

use objc2_foundation::{NSDictionary, NSNumber, NSString};

/// CGDirectDisplayID - unique identifier for a physical display
pub type CGDirectDisplayID = u32;

/// Main display ID constant
pub const CG_MAIN_DISPLAY_ID: CGDirectDisplayID = 0;

/// Core Graphics function pointers loaded via dlopen
pub struct CoreGraphicsFunctions {
    /// Function pointer to `CGMainDisplayID()` — returns the display ID of the main display
    cg_main_display_id: unsafe extern "C" fn() -> CGDirectDisplayID,
    /// Loaded library handle — kept alive to prevent unloading the function pointers
    #[allow(dead_code)]
    lib: libloading::Library,
}

impl CoreGraphicsFunctions {
    /// Load Core Graphics functions via dlopen
    pub fn load() -> Result<Arc<Self>, String> {
        unsafe {
            // Load ApplicationServices framework (which includes CoreGraphics)
            let lib = crate::desktop::open_first_lib(&[
                "/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices",
            ])
            .ok_or_else(|| "Failed to load ApplicationServices framework".to_string())?;

            // Load display functions
            let cg_main_display_id = *lib
                .get(b"CGMainDisplayID\0")
                .map_err(|e| format!("CGMainDisplayID not found: {}", e))?;

            Ok(Arc::new(Self {
                cg_main_display_id,
                lib,
            }))
        }
    }

    /// Get the main display ID
    pub fn main_display_id(&self) -> CGDirectDisplayID {
        unsafe { (self.cg_main_display_id)() }
    }
}

/// Extract CGDirectDisplayID from NSScreen's deviceDescription
///
/// The deviceDescription dictionary contains a "NSScreenNumber" key
/// which maps to the CGDirectDisplayID for that screen.
pub fn get_display_id_from_screen(screen: &objc2_app_kit::NSScreen) -> Option<CGDirectDisplayID> {
    unsafe {
        use objc2::msg_send;

        // Get deviceDescription dictionary
        let device_description: *const NSDictionary<NSString, objc2_foundation::NSObject> =
            msg_send![screen, deviceDescription];

        if device_description.is_null() {
            return None;
        }

        // Get "NSScreenNumber" key
        let key = NSString::from_str("NSScreenNumber");
        let value: *const objc2_foundation::NSObject =
            msg_send![device_description, objectForKey: &*key];

        if value.is_null() {
            return None;
        }

        // Cast to NSNumber and extract u32 (value is known non-null from check above)
        let ns_number = value as *const NSNumber;
        let display_id: u32 = msg_send![ns_number, unsignedIntValue];
        Some(display_id)
    }
}

/// Compute a stable hash for a monitor based on its properties
///
/// This hash can be used to identify the same physical monitor across sessions,
/// even if the index changes (e.g., monitors were plugged/unplugged).
pub fn compute_monitor_hash(
    display_id: CGDirectDisplayID,
    bounds: objc2_foundation::NSRect,
) -> u64 {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    let mut hasher = DefaultHasher::new();

    // Hash display ID (stable across sessions for the same physical monitor)
    display_id.hash(&mut hasher);

    // Hash bounds dimensions (width, height) using to_bits() to preserve exact f64 representation
    // We don't hash position because it can change when monitors are rearranged
    bounds.size.width.to_bits().hash(&mut hasher);
    bounds.size.height.to_bits().hash(&mut hasher);

    hasher.finish()
}
