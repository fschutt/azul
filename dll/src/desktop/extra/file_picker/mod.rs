//! Mobile file-picker dispatcher.
//!
//! Desktop builds keep using the existing `tfd`-backed synchronous API in
//! `layout/src/desktop/dialogs.rs::FileDialog`. Mobile builds need the
//! async pattern below — the OS picker is sheet-modal on iOS / intent-result on
//! Android, and blocking the UI thread waiting for a delegate callback
//! deadlocks the entire app.
//!
//! The pattern this module implements:
//!
//! 1. The user-facing `FileDialog::open_file_async(...)` in
//!    `layout/src/desktop/dialogs.rs` returns a [`FilePickerHandle`]. The
//!    handle holds an `Arc<Mutex<…>>` slot the OS callback writes into when
//!    the picker dismisses. azul-layout cannot call into this crate, so the
//!    dispatchers below are REGISTERED with it at startup
//!    ([`ensure_file_picker_backend`]), the way the camera and microphone
//!    capture backends are.
//!
//! 2. The user's layout / event callbacks poll the handle each frame via
//!    [`FilePickerHandle::poll`]. The first frame after the user picks /
//!    cancels, the poll returns a non-`Pending` status.
//!
//! 3. The platform backend's `apply_open_file` (iOS:
//!    `UIDocumentPickerViewController` with `asCopy=YES`; Android: an
//!    `Intent.ACTION_OPEN_DOCUMENT` round-trip via the JNI bridge) writes
//!    into the handle's slot when its delegate fires.
//!
//! This module owns the `apply_open_file` / `apply_save_file` /
//! `apply_open_directory` dispatchers and their registration. Each platform
//! submodule owns the actual OS plumbing; the handle type is azul-layout's.

use azul_css::{corety::OptionString, AzString, OptionStringVec};

// The handle and status types live in azul-layout (`desktop::dialogs`) so
// `FileDialog::open_file_async` — the user-facing entry point, which sits
// below this crate — can return them. Re-exported here so the platform
// submodules keep their `super::{FilePickerHandle, FilePickerStatus}`.
pub use azul_layout::desktop::dialogs::{FilePickerHandle, FilePickerStatus};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "ios")]
pub mod ios;

/// Install this module's dispatchers as azul-layout's async file-picker
/// backend, once. Mobile only: on the desktop `FileDialog::open_file_async`
/// answers synchronously through `tfd` and must NOT be routed here (the
/// non-mobile arms of the dispatchers below answer `Cancelled` without
/// showing anything). Called from the shared per-frame layout pass, like
/// `camera::ensure_camera_backend`, so it is in place before the first
/// callback that could ask for a picker.
pub fn ensure_file_picker_backend() {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            crate::plog_info!("[file_picker] registering the async OS picker backend");
            azul_layout::desktop::dialogs::register_file_picker_backend(
                azul_layout::desktop::dialogs::FilePickerBackend {
                    open_file: apply_open_file,
                    save_file: apply_save_file,
                    open_directory: apply_open_directory,
                },
            );
        });
    }
}

/// Open-file request. On mobile this fires off the platform picker and
/// returns immediately with a `Pending` handle. On non-mobile this is
/// never called — desktop keeps using the synchronous `tfd` path.
#[allow(unused_variables)] // every cfg arm consumes the inputs
pub fn apply_open_file(
    title: AzString,
    default_path: OptionString,
    filter_descriptors: OptionStringVec,
    allow_multiple: bool,
) -> FilePickerHandle {
    let handle = FilePickerHandle::new_pending();
    #[cfg(target_os = "ios")]
    ios::dispatch_open_file(
        handle.clone(),
        title,
        default_path,
        filter_descriptors,
        allow_multiple,
    );
    #[cfg(target_os = "android")]
    android::dispatch_open_file(
        handle.clone(),
        title,
        default_path,
        filter_descriptors,
        allow_multiple,
    );
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        handle.set_status(FilePickerStatus::Cancelled);
    }
    handle
}

/// Save-file request. iOS: `UIDocumentPickerViewController.initForExportingURLs`.
/// Android: `Intent.ACTION_CREATE_DOCUMENT`. Desktop keeps using `tfd`.
#[allow(unused_variables)]
pub fn apply_save_file(title: AzString, default_path: OptionString) -> FilePickerHandle {
    let handle = FilePickerHandle::new_pending();
    #[cfg(target_os = "ios")]
    ios::dispatch_save_file(handle.clone(), title, default_path);
    #[cfg(target_os = "android")]
    android::dispatch_save_file(handle.clone(), title, default_path);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        handle.set_status(FilePickerStatus::Cancelled);
    }
    handle
}

/// Directory-picker. iOS: `UIDocumentPickerViewController` with
/// `UTType.folder`. Android: `Intent.ACTION_OPEN_DOCUMENT_TREE` (API 21+).
#[allow(unused_variables)]
pub fn apply_open_directory(title: AzString, default_path: OptionString) -> FilePickerHandle {
    let handle = FilePickerHandle::new_pending();
    #[cfg(target_os = "ios")]
    ios::dispatch_open_directory(handle.clone(), title, default_path);
    #[cfg(target_os = "android")]
    android::dispatch_open_directory(handle.clone(), title, default_path);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        handle.set_status(FilePickerStatus::Cancelled);
    }
    handle
}
