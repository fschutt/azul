//! What the platform must have before the engine starts any I/O.
//!
//! Android: the crates under iroh that find the network (the DNS resolver, the list of network
//! interfaces) ask `ndk-context` for the JavaVM and the activity, and `ndk-context` panics when nobody
//! has told it yet ("android context was not initialized"). A release build is `panic = abort`, so the
//! process dies at once, with no way for the app to answer. `android-activity` tells `ndk-context` just
//! before it calls `android_main`, and the shell publishes the activity at the start of `android_main`
//! (`shell2::android::activity_ptr`). Anything that binds an endpoint earlier, such as a library
//! constructor (it runs at `System.loadLibrary`) or `main`, would reach those crates too early, so
//! `bind` looks first and answers with an error that the app can read.

/// What the engine asks of the platform.
pub(super) trait Platform {
    /// Whether the Android context (the JavaVM and the activity) exists. Always true off Android.
    fn android_context_ready(&self) -> bool;
}

/// The platform this process runs on.
pub(super) struct OsPlatform;

impl Platform for OsPlatform {
    #[cfg(all(target_os = "android", feature = "android-activity"))]
    fn android_context_ready(&self) -> bool {
        !crate::desktop::shell2::android::activity_ptr().is_null()
    }

    // Off Android there is no context to wait for; on Android without the NativeActivity glue nobody
    // here knows where the context comes from, so nothing is refused.
    #[cfg(not(all(target_os = "android", feature = "android-activity")))]
    fn android_context_ready(&self) -> bool {
        true
    }
}

/// The error of a bind that came before the Android context existed.
pub(super) const CONTEXT_NOT_READY: &str = "the Android context is not ready: IrohEndpoint::bind was \
    called before the app has a window (from the library's constructor, or from main); call it after \
    the window exists, from a callback such as the first layout callback or a button press";

/// `Ok` when the engine may start I/O, the diagnostic when it may not.
pub(super) fn require_context(_platform: &dyn Platform) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
#[path = "platform_test.rs"]
mod tests;
