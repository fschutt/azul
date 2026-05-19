//! Mobile file-picker dispatcher.
//!
//! Desktop builds keep using the existing `tfd`-backed synchronous API in
//! `layout/src/desktop/dialogs.rs::FileDialog`. Mobile builds need the
//! async pattern described in `scripts/research/04_system_integration.md`
//! §1.7 Option B — the OS picker is sheet-modal on iOS / intent-result on
//! Android, and blocking the UI thread waiting for a delegate callback
//! deadlocks the entire app.
//!
//! The pattern this module implements:
//!
//! 1. The user-facing `FileDialog::open_file_async(...)` (added in a
//!    follow-up tick to `layout/src/desktop/dialogs.rs`) returns a
//!    [`FilePickerHandle`]. The handle holds an `Arc<Mutex<…>>` slot the
//!    OS callback writes into when the picker dismisses.
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
//! This module owns the cross-platform handle type and the
//! `apply_open_file` / `apply_save_file` / `apply_open_directory`
//! dispatchers. Each platform submodule owns the actual OS plumbing.

use alloc::sync::Arc;
use std::sync::Mutex;

use azul_css::{corety::OptionString, AzString, OptionStringVec, StringVec};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "ios")]
pub mod ios;

/// Result of polling a [`FilePickerHandle`]. Mirrors the W3C
/// `showOpenFilePicker()` promise shape so the future web backend lands
/// without API churn.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FilePickerStatus {
    /// Picker is still on-screen; no user action yet.
    Pending,
    /// User dismissed the picker without selecting anything. Maps to the
    /// W3C `<input type="file">` cancel semantics (an empty selection).
    Cancelled,
    /// Single-file picker resolved. `OptionString::None` is impossible
    /// here — present for FFI shape parity with the desktop `open_file`
    /// return.
    Selected { path: OptionString },
    /// Multi-file picker resolved. Empty vec means the user dismissed
    /// without picking — equivalent to `Cancelled`.
    SelectedMultiple { paths: StringVec },
    /// Platform-level error (sandbox denial, intent failure, …). The
    /// message is user-presentable, the caller is expected to surface it.
    Error { message: AzString },
}

/// Shared state behind [`FilePickerHandle`]. Held in an `Arc<Mutex<…>>` so
/// the OS delegate / activity-result handler can write into it from the
/// UI thread while the layout callback reads from the engine thread.
#[derive(Debug)]
struct FilePickerInner {
    status: FilePickerStatus,
}

impl FilePickerInner {
    fn new() -> Self {
        Self {
            status: FilePickerStatus::Pending,
        }
    }
}

/// Opaque handle the user holds across event-loop ticks.
///
/// `#[repr(C)]` so the FFI surface sees a stable layout. The underlying
/// `Arc<Mutex<…>>` is reference-counted so cloning the handle is cheap;
/// every clone observes the same status updates.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct FilePickerHandle {
    inner: ArcMutexFilePickerInner,
}

/// Helper newtype around `Arc<Mutex<FilePickerInner>>`. Lives behind one
/// indirection so the public [`FilePickerHandle`] layout stays `repr(C)`
/// while the implementation can grow without breaking the wire ABI.
#[derive(Debug, Clone)]
#[repr(transparent)]
struct ArcMutexFilePickerInner(Arc<Mutex<FilePickerInner>>);

impl FilePickerHandle {
    /// Construct a fresh handle in `Pending` state. The platform backend
    /// retains a clone, fills in the status on user dismissal, and drops
    /// its clone — at which point only the user-side handle remains.
    pub fn new_pending() -> Self {
        Self {
            inner: ArcMutexFilePickerInner(Arc::new(Mutex::new(FilePickerInner::new()))),
        }
    }

    /// Sync read of the current status. Returns a clone so the caller can
    /// destructure without holding the mutex.
    pub fn poll(&self) -> FilePickerStatus {
        match self.inner.0.lock() {
            Ok(g) => g.status.clone(),
            Err(_) => FilePickerStatus::Error {
                message: AzString::from("file picker mutex poisoned"),
            },
        }
    }

    /// Platform-backend write path. Replaces the slot with the latest
    /// status. Idempotent — repeated writes from a flaky delegate keep
    /// the most recent value.
    pub fn set_status(&self, next: FilePickerStatus) {
        if let Ok(mut g) = self.inner.0.lock() {
            g.status = next;
        }
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
