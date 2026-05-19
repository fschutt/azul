//! Android file picker via Storage Access Framework — currently a stub.
//!
//! Final shape (queued for the next tick):
//!
//! 1. JNI bridge to a thin Java shim (`scripts/android/AzulFilePicker.java`,
//!    same pattern as `NativeGestureBridge.java`). The shim exposes
//!    `pickDocument(long nativePtr, String mimeType, boolean allowMultiple)`
//!    that calls `Activity.startActivityForResult(Intent(
//!    ACTION_OPEN_DOCUMENT).addCategory(CATEGORY_OPENABLE).setType(mimeType))`.
//! 2. The shim's `onActivityResult` reads `data.getData()` /
//!    `data.getClipData()`, converts each URI to a cached `file://` path
//!    (`ContentResolver.openInputStream` → write into `getCacheDir()`),
//!    then calls back into Rust via
//!    `Java_com_azul_picker_AzulFilePicker_nativeOnResult(nativePtr,
//!    pathArray)`.
//! 3. The native callback finds the `FilePickerHandle` by `nativePtr`
//!    (stored in a thread-local registry keyed by request code) and
//!    writes the resulting `FilePickerStatus`.
//!
//! MIME filter mapping from `filter_descriptors`:
//! - `*.png` → `image/png`
//! - `*.jpg`, `*.jpeg` → `image/jpeg`
//! - `*.pdf` → `application/pdf`
//! - any unknown extension → `application/octet-stream`
//!
//! `EXTRA_MIME_TYPES` carries the full array when multiple types are
//! requested. Glob filters (`*.tar.gz`) cannot be expressed in SAF and
//! the platform forces us to fall back to `*/*`.
//!
//! `AndroidManifest.xml` requires nothing for SAF itself — the picker
//! grants per-URI read permission via the intent flags. We *would* need
//! `READ_MEDIA_IMAGES` / `READ_MEDIA_VIDEO` / `READ_MEDIA_AUDIO` if we
//! ever offered a `MediaStore`-based gallery picker (a P6 expansion,
//! separate API surface).

use azul_css::{corety::OptionString, AzString, OptionStringVec};

use super::{FilePickerHandle, FilePickerStatus};

#[allow(unused_variables)]
pub fn dispatch_open_file(
    handle: FilePickerHandle,
    title: AzString,
    default_path: OptionString,
    filter_descriptors: OptionStringVec,
    allow_multiple: bool,
) {
    // TODO(P1.3+): JNI into AzulFilePicker.pickDocument + capture the
    // activity-result callback into `handle`.
    handle.set_status(FilePickerStatus::Cancelled);
}

#[allow(unused_variables)]
pub fn dispatch_save_file(handle: FilePickerHandle, title: AzString, default_path: OptionString) {
    // TODO(P1.3+): Intent.ACTION_CREATE_DOCUMENT (a separate
    // shim method) — same activity-result round-trip.
    handle.set_status(FilePickerStatus::Cancelled);
}

#[allow(unused_variables)]
pub fn dispatch_open_directory(
    handle: FilePickerHandle,
    title: AzString,
    default_path: OptionString,
) {
    // TODO(P1.3+): Intent.ACTION_OPEN_DOCUMENT_TREE — returns a
    // `tree://` URI. We persist it via
    // `takePersistableUriPermission` so subsequent launches can
    // re-open without re-prompting.
    handle.set_status(FilePickerStatus::Cancelled);
}
