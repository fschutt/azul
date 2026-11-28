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
//! 1. The user-facing resumable `FileDialog::open_file(...)` in `layout/src/desktop/dialogs.rs`
//!    returns a [`FilePickerHandle`]. The handle holds an `Arc<Mutex<…>>` slot the OS callback
//!    writes into when the picker dismisses. azul-layout cannot call into this crate, so the
//!    dispatchers below are REGISTERED with it at startup ([`ensure_file_picker_backend`]), the way
//!    the camera and microphone capture backends are.
//!
//! 2. The user's layout / event callbacks poll the handle each frame via
//!    [`FilePickerHandle::poll`]. The first frame after the user picks / cancels, the poll returns
//!    a non-`Pending` status.
//!
//! 3. The platform backend's `apply_open_file` (iOS: `UIDocumentPickerViewController` with
//!    `asCopy=YES`; Android: an `Intent.ACTION_OPEN_DOCUMENT` round-trip via the JNI bridge) writes
//!    into the handle's slot when its delegate fires.
//!
//! This module owns the `apply_open_file` / `apply_save_file` /
//! `apply_open_directory` dispatchers and their registration. Each platform
//! submodule owns the actual OS plumbing; the handle type is azul-layout's.

use azul_css::{corety::OptionString, AzString, OptionStringVec};
// The handle and status types live in azul-layout (`desktop::dialogs`) so
// `FileDialog::open_file` — the user-facing entry point, which sits
// below this crate — can return them. Re-exported here so the platform
// submodules keep their `super::{FilePickerHandle, FilePickerStatus}`.
pub use azul_layout::desktop::dialogs::{FilePickerHandle, FilePickerStatus};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "ios")]
pub mod ios;

/// Install this module's dispatchers as azul-layout's async file-picker
/// backend, once. Mobile only: on the desktop `FileDialog::open_file`
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
                    open_folder: apply_open_folder,
                    export_bytes: apply_export_bytes,
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

/// Save-file request. A phone has no save dialog behind it YET (iOS: `UIDocumentPickerViewController`
/// exporting, Android: `ACTION_CREATE_DOCUMENT` with a write-through target: ledger WP-F step 3a), and
/// until it has, the request FAILS with the reason: it must not show a dialog whose result cannot be
/// written (Android's used to create an empty document and hand the app a cache copy of it, iOS's
/// answered "cancelled" to every request). Desktop keeps using `tfd`.
#[allow(unused_variables)]
pub fn apply_save_file(title: AzString, default_path: OptionString) -> FilePickerHandle {
    let handle = FilePickerHandle::new_pending();
    #[cfg(target_os = "ios")]
    fail(&handle, IOS_SAVE_UNAVAILABLE);
    #[cfg(target_os = "android")]
    fail(&handle, ANDROID_SAVE_UNAVAILABLE);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        handle.set_status(FilePickerStatus::Cancelled);
    }
    handle
}

/// Directory-picker. iOS: `UIDocumentPickerViewController` with `UTType.folder`. Android: a SAF tree
/// (`ACTION_OPEN_DOCUMENT_TREE`) is not a folder a path can name, so until the document layer is behind
/// it (`FileDialog::open_folder` is the call for it) the request FAILS with the reason instead of showing a dialog whose answer
/// was thrown away (the tree URI was opened as a file and every pick came back "cancelled").
#[allow(unused_variables)]
pub fn apply_open_directory(title: AzString, default_path: OptionString) -> FilePickerHandle {
    let handle = FilePickerHandle::new_pending();
    #[cfg(target_os = "ios")]
    ios::dispatch_open_directory(handle.clone(), title, default_path);
    #[cfg(target_os = "android")]
    fail(&handle, ANDROID_FOLDER_UNAVAILABLE);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        handle.set_status(FilePickerStatus::Cancelled);
    }
    handle
}

/// Folder request, `FileDialog::open_folder`: a folder the person GRANTS and the app can use again, answered as the locator
/// `Documents` takes. Android: the system's tree picker, whose grant is kept (`takePersistableUriPermission`) so that the folder is
/// listed in `Documents::places()` after a restart. iOS: the document picker's folder, a security-scoped path (the same picker as
/// `apply_open_directory`: a path IS the locator there).
#[allow(unused_variables)]
pub fn apply_open_folder(title: AzString, default_path: OptionString) -> FilePickerHandle {
    let handle = FilePickerHandle::new_pending();
    #[cfg(target_os = "ios")]
    ios::dispatch_open_directory(handle.clone(), title, default_path);
    #[cfg(target_os = "android")]
    android::dispatch_open_folder(handle.clone(), title, default_path);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        handle.set_status(FilePickerStatus::Cancelled);
    }
    handle
}

/// Export request, `FileDialog::export_bytes`: the system's "save as", the bytes written where the person chose, the answer the name
/// they were saved under (only after the write). Android: `ACTION_CREATE_DOCUMENT` and a write through the document it creates.
/// iOS: the export picker over a file of the bytes.
#[allow(unused_variables)]
pub fn apply_export_bytes(suggested_name: AzString, mime: AzString, bytes: azul_css::U8Vec) -> FilePickerHandle {
    let handle = FilePickerHandle::new_pending();
    #[cfg(target_os = "ios")]
    ios::dispatch_export(handle.clone(), suggested_name, mime, bytes);
    #[cfg(target_os = "android")]
    android::dispatch_export(handle.clone(), suggested_name, mime, bytes);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        handle.set_status(FilePickerStatus::Cancelled);
    }
    handle
}

/// Answer a request with its failure: the reason reaches the app (`error` of its result) and the log, so the
/// failure is never a silent "nothing happened".
#[cfg(any(target_os = "ios", target_os = "android"))]
fn fail(handle: &FilePickerHandle, reason: &str) {
    crate::plog_warn!("[file_picker] {reason}");
    handle.set_status(FilePickerStatus::Error(AzString::from(reason)));
}

/// Why a phone answers a save request with an error: said once, here, so the apps and the tests read the same words.
pub const IOS_SAVE_UNAVAILABLE: &str =
    "saving to a place the user chooses is not available on iOS yet (the request was not shown and nothing was written)";
/// See [`IOS_SAVE_UNAVAILABLE`].
pub const ANDROID_SAVE_UNAVAILABLE: &str =
    "saving to a place the user chooses is not available on Android yet (the request was not shown and nothing was written)";
/// Why Android answers a folder request with an error.
pub const ANDROID_FOLDER_UNAVAILABLE: &str =
    "a folder on Android is a permission grant, not a path: ask for it with FileDialog::open_folder (the request was not shown)";
