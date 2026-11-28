//! The files and folders the user owns: the backend `CallbackInfo::documents` hands out.
//!
//! The data model is `azul_core::documents`, the handle, the trait and the desktop backend are `azul_layout::documents` (and the
//! pure half of the phone's storage framework `azul_layout::documents_saf`). This module is the OS plumbing and the one function
//! that installs it into the App's services at the start:
//!
//! | | Android | iOS | macOS / Linux / Windows |
//! |---|---|---|---|
//! | a locator is | a path (the app's own storage, "All files access") or a `content://` document URI inside a folder the person granted | a path (the app's container; a bookmarked folder once its scope is open) | a path |
//! | the backend | `AndroidDocuments`: `ContentResolver` and `DocumentsContract` through JNI, every call on a worker with a deadline | the desktop's, over paths | the desktop's, over paths |
//! | places | the app's storage, every persisted tree grant, "All files" where the person gave the access | the app's container | the user's well-known folders |
//!
//! Nothing here is global state: the backend is a value in the App's `AppServices` (`documents`), and its call guard (the count of
//! calls that outlived their deadline) is a field of it.

use azul_layout::managers::app_services::AppServices;

#[cfg(all(target_os = "android", feature = "jni"))]
mod android;

/// Install this platform's backend into `services` (the App's), once, at the start (`App::run`). A desktop keeps the slot's default,
/// which is the backend over paths.
pub fn install(services: &AppServices) {
    #[cfg(all(target_os = "android", feature = "jni"))]
    services.documents.install(std::sync::Arc::new(android::AndroidDocuments::new()));
    #[cfg(not(all(target_os = "android", feature = "jni")))]
    {
        // the slot's default (paths) is the backend of this platform
        let _ = services;
    }
}
