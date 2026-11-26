//! What the app's own `Info.plist` says, for the APIs that stop the process when it does not.
//!
//! A bundled app that asks the system for a protected resource without the matching purpose
//! string in its `Info.plist` is not refused politely: the system terminates it (Apple, "Requesting
//! access to protected resources": the request fails "and might cause your app to crash"; for
//! motion data "your app will crash when it attempts to access motion data"). The engine's
//! contract for those APIs is "permission unavailable, with the reason", never a crash and never
//! a silent no-op, so each of them asks [`usage_key_blocks_request`] BEFORE it asks the system:
//! the answer names the missing key, and the diagnostic ([`missing_key_message`]) says what to add.
//!
//! An UNBUNDLED binary (a demo started from a terminal, no `.app`) has no `Info.plist` of its own:
//! on macOS the system attributes the request to the terminal, which can hold the permission, so
//! the key is only demanded of a bundle.

/// The keys the engine's protected APIs need (the purpose strings).
pub mod keys {
    /// `CMPedometer` / `CMMotionActivityManager` (and `CMAltimeter` on iOS).
    pub const MOTION: &str = "NSMotionUsageDescription";
    /// `AVCaptureDevice requestAccess` for video.
    pub const CAMERA: &str = "NSCameraUsageDescription";
    /// `AVCaptureDevice requestAccess` for audio.
    pub const MICROPHONE: &str = "NSMicrophoneUsageDescription";
}

/// Must the request NOT be made: the process is a bundle (`bundled`) and its `Info.plist` lacks the
/// purpose string (`has_key` false).
#[must_use]
pub const fn usage_key_blocks_request(bundled: bool, has_key: bool) -> bool {
    bundled && !has_key
}

/// The diagnostic for a request that was not made: what was asked, which key is missing.
#[must_use]
pub fn missing_key_message(what: &str, key: &str) -> String {
    format!(
        "{what} was not requested: this app's Info.plist has no {key}, and asking the system \
         without it stops the process. Add {key} (the text the user is shown) - for the apps \
         built by scripts/build-ios.sh that is `AZ_IOS_USAGE=\"{key}=<why>\"`."
    )
}

/// Whether the main bundle's `Info.plist` carries `key` (any value).
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[must_use]
pub fn has_key(key: &str) -> bool {
    use core::ffi::c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFBundleGetMainBundle() -> *const c_void;
        fn CFBundleGetValueForInfoDictionaryKey(
            bundle: *const c_void,
            key: *const c_void,
        ) -> *const c_void;
        fn CFStringCreateWithBytes(
            allocator: *const c_void,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            is_external_representation: u8,
        ) -> *const c_void;
        fn CFRelease(object: *const c_void);
    }
    /// `kCFStringEncodingUTF8`.
    const UTF8: u32 = 0x0800_0100;

    // SAFETY: CoreFoundation calls with a string made here and released here; the main bundle
    // and the value read from its dictionary are not owned (the Get rule).
    unsafe {
        let bundle = CFBundleGetMainBundle();
        if bundle.is_null() {
            return false;
        }
        let name = CFStringCreateWithBytes(
            core::ptr::null(),
            key.as_ptr(),
            key.len() as isize,
            UTF8,
            0,
        );
        if name.is_null() {
            return false;
        }
        let value = CFBundleGetValueForInfoDictionaryKey(bundle, name);
        CFRelease(name);
        !value.is_null()
    }
}

/// Whether this process runs from a bundle with an `Info.plist` of its own (it names a
/// `CFBundleIdentifier`); a bare binary does not.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[must_use]
pub fn is_bundled() -> bool {
    has_key("CFBundleIdentifier")
}

/// Is a request for the resource behind `key` one that would stop this process? (Bundled, and no
/// purpose string.)
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[must_use]
pub fn request_would_stop_the_process(key: &str) -> bool {
    usage_key_blocks_request(is_bundled(), has_key(key))
}

#[cfg(test)]
#[path = "info_plist_test.rs"]
mod info_plist_test;
