//! Native OS dialog wrappers (message boxes, file open/save, color picker).
//!
//! Desktop targets back this with the `tfd` (tiny-file-dialogs) crate; on
//! Android / iOS every method is a no-op that returns the "cancelled / safe
//! default" answer (there is no equivalent of `tfd` on those platforms from
//! a pure-Rust crate, and `tfd 0.1.0` does not cross-compile for them
//! anyway). The public type surface is identical on every target so
//! consumer code keeps compiling.

use azul_css::{
    corety::OptionString,
    impl_option, impl_option_inner,
    props::basic::color::{ColorU, OptionColorU},
    AzString, OptionStringVec, StringVec,
};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tfd::{DefaultColorValue, MessageBoxIcon};

/// Static-method namespace for `tfd`-backed message-box dialogs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder field
pub struct MsgBox {
    pub _reserved: u8,
}

/// Static-method namespace for `tfd`-backed file dialogs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder field
pub struct FileDialog {
    pub _reserved: u8,
}

/// Static-method namespace for the `tfd`-backed color picker.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder field
pub struct ColorPickerDialog {
    pub _reserved: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum OkCancel {
    Ok,
    Cancel,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<tfd::OkCancel> for OkCancel {
    #[inline]
    fn from(e: tfd::OkCancel) -> Self {
        match e {
            tfd::OkCancel::Ok => Self::Ok,
            tfd::OkCancel::Cancel => Self::Cancel,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<OkCancel> for tfd::OkCancel {
    #[inline]
    fn from(e: OkCancel) -> Self {
        match e {
            OkCancel::Ok => Self::Ok,
            OkCancel::Cancel => Self::Cancel,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum YesNo {
    Yes,
    No,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<YesNo> for tfd::YesNo {
    #[inline]
    fn from(e: YesNo) -> Self {
        match e {
            YesNo::Yes => Self::Yes,
            YesNo::No => Self::No,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<tfd::YesNo> for YesNo {
    #[inline]
    fn from(e: tfd::YesNo) -> Self {
        match e {
            tfd::YesNo::Yes => Self::Yes,
            tfd::YesNo::No => Self::No,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum MsgBoxIcon {
    Info,
    Warning,
    Error,
    Question,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<MsgBoxIcon> for MessageBoxIcon {
    #[inline]
    fn from(e: MsgBoxIcon) -> Self {
        match e {
            MsgBoxIcon::Info => Self::Info,
            MsgBoxIcon::Warning => Self::Warning,
            MsgBoxIcon::Error => Self::Error,
            MsgBoxIcon::Question => Self::Question,
        }
    }
}

impl Default for MsgBox {
    fn default() -> Self {
        Self::new()
    }
}

impl MsgBox {
    /// Returns a zero-initialised namespace handle. The struct itself carries
    /// no state — instances exist only so the FFI layer can hang static
    /// methods off the type.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// "Ok" message box — title, message, icon. Quotes are stripped from the
    /// message to work around `tfd` misinterpreting them as shell metacharacters
    /// on some platforms.
    // owned C-ABI dialog types (AzString/MsgBoxIcon) are passed by value per the azul FFI
    // / api.json convention; taking them by reference would break the exported signature.
    #[allow(clippy::needless_pass_by_value)]
    pub fn ok(title: AzString, message: AzString, icon: MsgBoxIcon) {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut msg = message.as_str().to_string();
            msg = msg.replace('\"', "");
            msg = msg.replace('\'', "");
            tfd::MessageBox::new(title.as_str(), &msg)
                .with_icon(icon.into())
                .run_modal();
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, message, icon);
        }
    }

    /// "Ok / Cancel" message box — title, message, icon, default button.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn ok_cancel(
        title: AzString,
        message: AzString,
        icon: MsgBoxIcon,
        default: OkCancel,
    ) -> OkCancel {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            tfd::MessageBox::new(title.as_str(), message.as_str())
                .with_icon(icon.into())
                .run_modal_ok_cancel(default.into())
                .into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, message, icon);
            default
        }
    }

    /// "Yes / No" message box — title, message, icon, default button.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn yes_no(title: AzString, message: AzString, icon: MsgBoxIcon, default: YesNo) -> YesNo {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            tfd::MessageBox::new(title.as_str(), message.as_str())
                .with_icon(icon.into())
                .run_modal_yes_no(default.into())
                .into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, message, icon);
            default
        }
    }

    /// Convenience: "Ok" message box with the title "Info" and an info icon.
    pub fn info(content: AzString) {
        Self::ok(AzString::from("Info"), content, MsgBoxIcon::Info);
    }
}

impl Default for ColorPickerDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorPickerDialog {
    /// Returns a zero-initialised namespace handle. Static-only — the struct
    /// is just a hook for the FFI layer.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Opens the default color picker dialog. Returns `None` if cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn open(title: AzString, default_value: OptionColorU) -> OptionColorU {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let rgb = default_value
                .into_option()
                .map_or([0, 0, 0], |c| [c.r, c.g, c.b]);
            let default_color = DefaultColorValue::RGB(rgb);
            let result = tfd::ColorChooser::new(title.as_str())
                .with_default_color(default_color)
                .run_modal();
            match result {
                Some(r) => OptionColorU::Some(ColorU {
                    r: r.1[0],
                    g: r.1[1],
                    b: r.1[2],
                    a: ColorU::ALPHA_OPAQUE,
                }),
                None => OptionColorU::None,
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = title;
            default_value
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct FileTypeList {
    pub document_types: StringVec,
    pub document_descriptor: AzString,
}

impl_option!(
    FileTypeList,
    OptionFileTypeList,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd]
);

/// Apply a [`FileTypeList`] filter to a `tfd::FileDialog`.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
// consumes the FileTypeList forwarded from the by-value FFI file-dialog API.
#[allow(clippy::needless_pass_by_value)]
fn apply_filter(mut dialog: tfd::FileDialog, filter: FileTypeList) -> tfd::FileDialog {
    let v = filter.document_types.clone().into_library_owned_vec();
    let patterns: Vec<&str> = v.iter().map(AzString::as_str).collect();
    dialog = dialog.with_filter(&patterns, filter.document_descriptor.as_str());
    dialog
}

impl Default for FileDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDialog {
    /// Returns a zero-initialised namespace handle. Static-only — the struct
    /// is just a hook for the FFI layer.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Open a single file. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_file(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
    ) -> OptionString {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            if let Some(filter) = filter_list.into_option() {
                dialog = apply_filter(dialog, filter);
            }
            dialog.open_file().map(AzString::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path, filter_list);
            OptionString::None
        }
    }

    /// Open a directory. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_directory(title: AzString, default_path: OptionString) -> OptionString {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            dialog.select_folder().map(AzString::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path);
            OptionString::None
        }
    }

    /// Open multiple files. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_multiple_files(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
    ) -> OptionStringVec {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str()).with_multiple_selection(true);
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            if let Some(filter) = filter_list.into_option() {
                dialog = apply_filter(dialog, filter);
            }
            dialog.open_files().map(StringVec::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path, filter_list);
            OptionStringVec::None
        }
    }

    /// Save file dialog. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn save_file(title: AzString, default_path: OptionString) -> OptionString {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            dialog.save_file().map(AzString::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path);
            OptionString::None
        }
    }
}

// ============================================================================
// Async file picker
// ============================================================================
//
// `FileDialog::open_file` above BLOCKS until the user answers. That is fine
// on the desktop (tfd runs a nested modal loop) and fatal on mobile: the iOS
// document picker is sheet-modal and reports through a delegate on the main
// thread, Android's is an `Intent` whose result arrives at
// `onActivityResult` — blocking the UI thread waiting for either deadlocks
// the app. So the mobile shape is a HANDLE the caller polls from its normal
// callbacks, and the desktop answers the same handle synchronously so one
// application code path works everywhere.
//
// The OS plumbing lives in the dll (`desktop/extra/file_picker/{ios,android}`)
// and cannot be called from here — azul-layout sits below azul-dll — so it is
// REGISTERED, the same way the camera and microphone capture backends are:
// the dll installs a [`FilePickerBackend`] at startup, and
// [`FileDialog::open_file_async`] dispatches to it when one is present.

use std::sync::{Arc, Mutex, OnceLock};

/// Result of polling a [`FilePickerHandle`]. Mirrors the `W3C`
/// `showOpenFilePicker()` promise shape so a web backend lands without API
/// churn.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FilePickerStatus {
    /// Picker is still on-screen; no user action yet.
    Pending,
    /// User dismissed the picker without selecting anything. Maps to the
    /// `W3C` `<input type="file">` cancel semantics (an empty selection).
    Cancelled,
    /// Single-file picker resolved: the chosen path.
    Selected(AzString),
    /// Multi-file picker resolved. Empty vec means the user dismissed
    /// without picking — equivalent to `Cancelled`.
    SelectedMultiple(StringVec),
    /// Platform-level error (sandbox denial, intent failure, no backend on
    /// this platform, …). The message is user-presentable; the caller is
    /// expected to surface it.
    Error(AzString),
}

/// Shared state behind [`FilePickerHandle`].
///
/// Held in an `Arc<Mutex<…>>` so
/// the OS delegate / activity-result handler can write into it from the UI
/// thread while the layout callback reads it from the engine thread.
#[derive(Debug)]
struct FilePickerInner {
    status: FilePickerStatus,
}

type SharedInner = Mutex<FilePickerInner>;

/// Opaque handle the user holds across event-loop ticks.
///
/// The FFI shape of every engine-resource handle (`Db`, `Pdf`, …): a
/// pointer plus a destructor flag, `#[repr(C)]`. Unlike those, this one is
/// REFERENCE-COUNTED — `ptr` is an `Arc<Mutex<FilePickerInner>>` and every
/// handle owns one strong count — because the OS backend keeps a clone and
/// writes the answer into it later, possibly after the user dropped theirs.
/// A shallow, non-owning clone would be a use-after-free waiting for the
/// picker to dismiss. A null `ptr` (the `Default`) polls as an `Error`.
#[derive(Debug)]
#[repr(C)]
pub struct FilePickerHandle {
    /// `Arc::into_raw` of the shared slot; one strong count per handle.
    pub ptr: *const core::ffi::c_void,
    /// `true` when dropping this handle releases its strong count — every
    /// live handle; `false` only for the null `Default`.
    pub run_destructor: bool,
}

// SAFETY: the only thing behind `ptr` is an `Arc<Mutex<FilePickerInner>>`,
// which is `Send + Sync`; the handle is that `Arc` with its type erased.
unsafe impl Send for FilePickerHandle {}
unsafe impl Sync for FilePickerHandle {}

impl FilePickerHandle {
    /// A fresh handle in `Pending` state. The platform backend retains a
    /// clone, fills in the status on user dismissal, and drops its clone — at
    /// which point only the user-side handle remains.
    #[must_use]
    pub fn new_pending() -> Self {
        Self::with_status(FilePickerStatus::Pending)
    }

    /// A handle that is ALREADY answered — what the desktop returns after its
    /// synchronous dialog, and what a platform with no picker returns with an
    /// `Error`. The first `poll` sees the answer.
    #[must_use]
    pub fn with_status(status: FilePickerStatus) -> Self {
        let arc: Arc<SharedInner> = Arc::new(Mutex::new(FilePickerInner { status }));
        Self {
            ptr: Arc::into_raw(arc).cast::<core::ffi::c_void>(),
            run_destructor: true,
        }
    }

    /// The shared slot, or `None` for the null `Default` handle.
    const fn inner(&self) -> Option<&SharedInner> {
        if self.ptr.is_null() {
            return None;
        }
        // SAFETY: a non-null `ptr` came from `Arc::into_raw` in `with_status`
        // and this handle holds a strong count, so the allocation is alive
        // for as long as `&self` is.
        Some(unsafe { &*self.ptr.cast::<SharedInner>() })
    }

    /// Sync read of the current status. Returns a clone so the caller can
    /// destructure without holding the mutex.
    #[must_use]
    pub fn poll(&self) -> FilePickerStatus {
        match self.inner().map(Mutex::lock) {
            Some(Ok(g)) => g.status.clone(),
            Some(Err(_)) => FilePickerStatus::Error(AzString::from("file picker mutex poisoned")),
            None => FilePickerStatus::Error(AzString::from(
                "null file picker handle (a Default, not one a FileDialog returned)",
            )),
        }
    }

    /// `true` once the picker has been answered (anything but `Pending`).
    #[must_use]
    pub fn is_done(&self) -> bool {
        !matches!(self.poll(), FilePickerStatus::Pending)
    }

    /// Platform-backend write path. Replaces the slot with the latest
    /// status. Idempotent — repeated writes from a flaky delegate keep the
    /// most recent value.
    pub fn set_status(&self, next: FilePickerStatus) {
        if let Some(Ok(mut g)) = self.inner().map(Mutex::lock) {
            g.status = next;
        }
    }
}

impl Clone for FilePickerHandle {
    /// Another owner of the SAME slot — every clone observes the same status
    /// updates. Increments the strong count; the clone releases it on drop.
    fn clone(&self) -> Self {
        if self.ptr.is_null() {
            return Self::default();
        }
        // SAFETY: see `inner`; incrementing while we hold a count is sound.
        unsafe { Arc::increment_strong_count(self.ptr.cast::<SharedInner>()) };
        Self {
            ptr: self.ptr,
            run_destructor: true,
        }
    }
}

impl Default for FilePickerHandle {
    /// The null handle: polls as an `Error`, clones to another null, drops
    /// to nothing. What the FFI hands out for "no handle".
    fn default() -> Self {
        Self {
            ptr: core::ptr::null(),
            run_destructor: false,
        }
    }
}

impl Drop for FilePickerHandle {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            // SAFETY: this handle's own strong count, taken in
            // `with_status` / `clone`, released exactly once here.
            drop(unsafe { Arc::from_raw(self.ptr.cast::<SharedInner>()) });
            self.ptr = core::ptr::null();
            self.run_destructor = false;
        }
    }
}

/// The OS file-picker plumbing a platform shell installs at startup — the
/// async equivalent of the `tfd` calls above.
///
/// Each function must return
/// IMMEDIATELY with a `Pending` handle it later resolves from the OS callback.
#[derive(Debug, Clone, Copy)]
pub struct FilePickerBackend {
    /// `(title, default_path, filter patterns, allow_multiple)`.
    pub open_file: fn(AzString, OptionString, OptionStringVec, bool) -> FilePickerHandle,
    /// `(title, default_path)`.
    pub save_file: fn(AzString, OptionString) -> FilePickerHandle,
    /// `(title, default_path)`.
    pub open_directory: fn(AzString, OptionString) -> FilePickerHandle,
}

static FILE_PICKER_BACKEND: OnceLock<FilePickerBackend> = OnceLock::new();

/// Install the platform's async file picker.
///
/// The first registration wins;
/// returns `false` when one was already installed (the shells register from
/// a `OnceLock`-guarded site, so that is a programming error, not a race to
/// paper over).
pub fn register_file_picker_backend(backend: FilePickerBackend) -> bool {
    FILE_PICKER_BACKEND.set(backend).is_ok()
}

/// Whether an async backend has been installed (i.e. whether the `*_async`
/// calls will go to the OS picker or answer synchronously / with an error).
#[must_use]
pub fn has_file_picker_backend() -> bool {
    FILE_PICKER_BACKEND.get().is_some()
}

/// The filter patterns of a [`FileTypeList`], in the shape the async
/// backends take: the descriptor is desktop-dialog chrome that neither
/// mobile picker displays.
fn filter_patterns(filter_list: OptionFileTypeList) -> OptionStringVec {
    filter_list.into_option().map(|f| f.document_types).into()
}

impl FileDialog {
    /// Open a file WITHOUT blocking: returns a [`FilePickerHandle`] to poll
    /// from a later callback (a timer, the next event, the layout callback).
    ///
    /// - iOS / Android: the OS picker is presented and the handle resolves
    ///   when its delegate / activity result fires.
    /// - Desktop: the synchronous dialog runs here (it is modal anyway) and
    ///   the handle comes back already answered, so the same polling code
    ///   sees `Selected` / `Cancelled` on its first `poll`.
    /// - A mobile build whose shell never registered a backend: `Error`,
    ///   immediately — never a handle that stays `Pending` forever.
    ///
    /// `allow_multiple` resolves to `SelectedMultiple` instead of `Selected`.
    #[must_use]
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_file_async(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
        allow_multiple: bool,
    ) -> FilePickerHandle {
        if let Some(backend) = FILE_PICKER_BACKEND.get() {
            return (backend.open_file)(
                title,
                default_path,
                filter_patterns(filter_list),
                allow_multiple,
            );
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let status = if allow_multiple {
                match Self::open_multiple_files(title, default_path, filter_list).into_option() {
                    Some(paths) => FilePickerStatus::SelectedMultiple(paths),
                    None => FilePickerStatus::Cancelled,
                }
            } else {
                match Self::open_file(title, default_path, filter_list).into_option() {
                    Some(path) => FilePickerStatus::Selected(path),
                    None => FilePickerStatus::Cancelled,
                }
            };
            FilePickerHandle::with_status(status)
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path, filter_list, allow_multiple);
            FilePickerHandle::with_status(no_backend_error())
        }
    }

    /// Save-file counterpart of [`Self::open_file_async`]; resolves to
    /// `Selected` with the chosen path.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn save_file_async(title: AzString, default_path: OptionString) -> FilePickerHandle {
        if let Some(backend) = FILE_PICKER_BACKEND.get() {
            return (backend.save_file)(title, default_path);
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let status = match Self::save_file(title, default_path).into_option() {
                Some(path) => FilePickerStatus::Selected(path),
                None => FilePickerStatus::Cancelled,
            };
            FilePickerHandle::with_status(status)
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path);
            FilePickerHandle::with_status(no_backend_error())
        }
    }

    /// Directory counterpart of [`Self::open_file_async`]; resolves to
    /// `Selected` with the chosen directory.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_directory_async(title: AzString, default_path: OptionString) -> FilePickerHandle {
        if let Some(backend) = FILE_PICKER_BACKEND.get() {
            return (backend.open_directory)(title, default_path);
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let status = match Self::open_directory(title, default_path).into_option() {
                Some(path) => FilePickerStatus::Selected(path),
                None => FilePickerStatus::Cancelled,
            };
            FilePickerHandle::with_status(status)
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path);
            FilePickerHandle::with_status(no_backend_error())
        }
    }
}

/// The answer on a platform with no synchronous dialog and no registered
/// async backend. An explicit error rather than `Cancelled`: a picker the
/// user never saw must not read as "the user declined".
#[cfg(any(target_os = "android", target_os = "ios"))]
fn no_backend_error() -> FilePickerStatus {
    FilePickerStatus::Error(AzString::from(
        "no file picker backend is registered on this platform (the shell must call \
             register_file_picker_backend at startup)",
    ))
}

/// Convenience shim: show a default "Info" message box.
pub fn msg_box(content: &str) {
    MsgBox::info(AzString::from(content));
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // Every dialog entry point in this file (`MsgBox::ok`, `FileDialog::open_file`,
    // `ColorPickerDialog::open`, `msg_box`, ...) ends in a `run_modal()` /
    // `open_file()` call that blocks on a native modal window. Calling one from a
    // test would hang the test binary forever (or shell out to zenity/kdialog on
    // a headless box), so they are NEVER invoked here. Instead they are covered by
    // a signature guard (below) that type-checks the FFI surface without running
    // it, and by the android/iOS no-op contract tests, which exercise the branch
    // that genuinely returns without showing a dialog.
    //
    // What IS exercised for real: the three const namespace constructors, the
    // `tfd` enum conversions, and `apply_filter` — a pure builder that never
    // opens anything.

    fn s(value: &str) -> AzString {
        AzString::from(value.to_string())
    }

    fn file_type_list(patterns: &[&str], descriptor: &str) -> FileTypeList {
        FileTypeList {
            document_types: StringVec::from_vec(patterns.iter().map(|p| s(p)).collect()),
            document_descriptor: s(descriptor),
        }
    }

    // ---------------------------------------------------------------------
    // Constructors: MsgBox::new / FileDialog::new / ColorPickerDialog::new
    // ---------------------------------------------------------------------

    #[test]
    fn namespace_handles_are_zero_initialised() {
        assert_eq!(MsgBox::new()._reserved, 0);
        assert_eq!(FileDialog::new()._reserved, 0);
        assert_eq!(ColorPickerDialog::new()._reserved, 0);
    }

    #[test]
    fn namespace_handles_are_const_evaluable() {
        // `new()` is `const fn`; if it ever stops being usable in a const context
        // the FFI/api.json static-namespace contract breaks. This fails to compile
        // rather than fails at runtime, which is the point.
        const MSG_BOX: MsgBox = MsgBox::new();
        const FILE_DIALOG: FileDialog = FileDialog::new();
        const COLOR_PICKER: ColorPickerDialog = ColorPickerDialog::new();

        assert_eq!(MSG_BOX._reserved, 0);
        assert_eq!(FILE_DIALOG._reserved, 0);
        assert_eq!(COLOR_PICKER._reserved, 0);
    }

    #[test]
    fn namespace_handles_default_matches_new() {
        assert_eq!(MsgBox::default(), MsgBox::new());
        assert_eq!(FileDialog::default(), FileDialog::new());
        assert_eq!(ColorPickerDialog::default(), ColorPickerDialog::new());
    }

    #[test]
    fn namespace_handles_are_stateless_single_byte_shims() {
        // These types are `#[repr(C)]` placeholders that the FFI layer hangs static
        // methods off. A field creeping in would silently change the C ABI.
        assert_eq!(size_of::<MsgBox>(), 1);
        assert_eq!(size_of::<FileDialog>(), 1);
        assert_eq!(size_of::<ColorPickerDialog>(), 1);
        assert_eq!(align_of::<MsgBox>(), 1);
        assert_eq!(align_of::<FileDialog>(), 1);
        assert_eq!(align_of::<ColorPickerDialog>(), 1);
    }

    #[test]
    fn namespace_handles_are_copy_and_hash_consistently() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        fn hash_of<T: Hash>(value: &T) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }

        let original = MsgBox::new();
        let copied = original; // Copy, not a move
        assert_eq!(original, copied);
        assert_eq!(hash_of(&original), hash_of(&copied));
        assert_eq!(hash_of(&MsgBox::new()), hash_of(&MsgBox::new()));
        assert_eq!(hash_of(&FileDialog::new()), hash_of(&FileDialog::new()));
        assert_eq!(
            hash_of(&ColorPickerDialog::new()),
            hash_of(&ColorPickerDialog::new())
        );
    }

    // ---------------------------------------------------------------------
    // Enum conversions to/from `tfd` (round-trip: encode == decode)
    // ---------------------------------------------------------------------

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn ok_cancel_round_trips_through_tfd() {
        for variant in [OkCancel::Ok, OkCancel::Cancel] {
            let encoded: tfd::OkCancel = variant.into();
            let decoded: OkCancel = encoded.into();
            assert_eq!(decoded, variant, "round-trip lost {variant:?}");
        }

        // ... and the other direction, exhaustively.
        assert_eq!(OkCancel::from(tfd::OkCancel::Ok), OkCancel::Ok);
        assert_eq!(OkCancel::from(tfd::OkCancel::Cancel), OkCancel::Cancel);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn yes_no_round_trips_through_tfd() {
        for variant in [YesNo::Yes, YesNo::No] {
            let encoded: tfd::YesNo = variant.into();
            let decoded: YesNo = encoded.into();
            assert_eq!(decoded, variant, "round-trip lost {variant:?}");
        }

        assert_eq!(YesNo::from(tfd::YesNo::Yes), YesNo::Yes);
        assert_eq!(YesNo::from(tfd::YesNo::No), YesNo::No);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn answer_enums_must_be_converted_by_variant_never_by_discriminant() {
        // azul declares `OkCancel { Ok, Cancel }` (Ok = 0) but tfd declares
        // `OkCancel { Cancel = 0, Ok = 1 }` — the discriminants are INVERTED.
        // Same story for YesNo. So a `transmute` or an `as`-cast in place of the
        // `From` impls would silently turn "Ok" into "Cancel", i.e. hand the caller
        // the exact opposite of what the user clicked. This test pins the mismatch
        // so nobody "optimises" the match arms into a cast.
        assert_eq!(OkCancel::Ok as u8, 0);
        assert_eq!(OkCancel::Cancel as u8, 1);
        assert_eq!(tfd::OkCancel::Ok as u8, 1);
        assert_eq!(tfd::OkCancel::Cancel as u8, 0);

        assert_eq!(YesNo::Yes as u8, 0);
        assert_eq!(YesNo::No as u8, 1);
        assert_eq!(tfd::YesNo::Yes as u8, 1);
        assert_eq!(tfd::YesNo::No as u8, 0);

        // The conversions must follow the variant, not the number.
        assert_eq!(tfd::OkCancel::from(OkCancel::Ok), tfd::OkCancel::Ok);
        assert_eq!(tfd::YesNo::from(YesNo::Yes), tfd::YesNo::Yes);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn msg_box_icon_maps_to_the_matching_tfd_icon() {
        let mapping = [
            (MsgBoxIcon::Info, MessageBoxIcon::Info),
            (MsgBoxIcon::Warning, MessageBoxIcon::Warning),
            (MsgBoxIcon::Error, MessageBoxIcon::Error),
            (MsgBoxIcon::Question, MessageBoxIcon::Question),
        ];
        for (ours, theirs) in mapping {
            assert_eq!(
                MessageBoxIcon::from(ours),
                theirs,
                "wrong icon for {ours:?}"
            );
        }

        // Injective: four distinct inputs must not collapse onto three icons.
        let encoded: Vec<MessageBoxIcon> = mapping
            .iter()
            .map(|(ours, _)| MessageBoxIcon::from(*ours))
            .collect();
        for (i, a) in encoded.iter().enumerate() {
            for b in encoded.iter().skip(i + 1) {
                assert_ne!(a, b, "two MsgBoxIcon variants map to the same tfd icon");
            }
        }
    }

    // ---------------------------------------------------------------------
    // apply_filter — the only non-modal logic in this file
    // ---------------------------------------------------------------------

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_with_no_patterns_does_not_panic() {
        let dialog = apply_filter(tfd::FileDialog::new("title"), file_type_list(&[], ""));
        assert!(dialog.filter_patterns().is_empty());
        assert_eq!(dialog.filter_description(), "");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_with_a_default_constructed_string_vec_does_not_panic() {
        // `StringVec::new()` is the empty/possibly-null-pointer case that
        // `into_library_owned_vec` has to survive.
        let filter = FileTypeList {
            document_types: StringVec::new(),
            document_descriptor: s("no types"),
        };
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);
        assert!(dialog.filter_patterns().is_empty());
        assert_eq!(dialog.filter_description(), "no types");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_preserves_patterns_verbatim_and_in_order() {
        let filter = file_type_list(&["*.png", "*.jpg", "*.png", ""], "Images");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        // Duplicates and the empty pattern survive: the filter is a pass-through,
        // not a set.
        assert_eq!(dialog.filter_patterns(), &["*.png", "*.jpg", "*.png", ""]);
        assert_eq!(dialog.filter_description(), "Images");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_preserves_unicode_patterns() {
        let patterns = [
            "*.图片",         // CJK
            "*.🎨",           // astral-plane emoji
            "*.مِلَف",          // RTL with combining marks
            "*.e\u{0301}xt",  // decomposed é — must not be normalised away
            "*.\u{200B}zwsp", // zero-width space
        ];
        let filter = file_type_list(&patterns, "Ünïcödé — файлы 🎨");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(dialog.filter_patterns(), &patterns);
        assert_eq!(dialog.filter_description(), "Ünïcödé — файлы 🎨");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_does_not_truncate_at_interior_nul_bytes() {
        // A NUL is a legal Rust `str` byte but terminates a C string. `apply_filter`
        // is pure Rust, so it must hand the bytes on intact rather than silently
        // cutting the pattern short (a truncation here would turn "*.png\0evil" into
        // a filter the caller never asked for).
        let filter = file_type_list(&["*.pn\0g", "\0", "a\u{1}b\u{7f}"], "desc\0ription");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(
            dialog.filter_patterns(),
            &["*.pn\0g", "\0", "a\u{1}b\u{7f}"]
        );
        assert_eq!(dialog.filter_description(), "desc\0ription");
        assert_eq!(dialog.filter_patterns()[0].len(), 6); // bytes kept, not cut at the NUL
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_passes_shell_metacharacters_through_unchanged() {
        // Documents the ACTUAL behaviour: unlike `MsgBox::ok` (which strips quotes
        // before handing the string to tfd), `apply_filter` sanitises nothing. If a
        // sanitisation step is ever added, this test should be updated deliberately
        // — it must not change by accident.
        let hostile = ["\"", "'", "$(id)", "`id`", "a;b", "x\ny", "--", "*"];
        let filter = file_type_list(&hostile, "\"quoted\" $(id)");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(dialog.filter_patterns(), &hostile);
        assert_eq!(dialog.filter_description(), "\"quoted\" $(id)");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_survives_a_huge_filter_list() {
        let patterns: Vec<String> = (0..2000).map(|i| format!("*.ext{i}")).collect();
        let descriptor = "d".repeat(64 * 1024);
        let filter = FileTypeList {
            document_types: StringVec::from_vec(
                patterns.iter().map(|p| s(p)).collect::<Vec<AzString>>(),
            ),
            document_descriptor: s(&descriptor),
        };

        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(dialog.filter_patterns().len(), 2000);
        assert_eq!(dialog.filter_patterns()[0], "*.ext0");
        assert_eq!(dialog.filter_patterns()[1999], "*.ext1999");
        assert_eq!(dialog.filter_description().len(), 64 * 1024);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_overwrites_rather_than_appends() {
        // tfd's `with_filter` assigns, so applying twice is last-write-wins. Worth
        // pinning: an `open_file` caller that expects the two lists to merge would
        // silently lose the first set of extensions.
        let dialog = tfd::FileDialog::new("title");
        let dialog = apply_filter(dialog, file_type_list(&["*.png"], "Images"));
        let dialog = apply_filter(dialog, file_type_list(&["*.txt"], "Text"));

        assert_eq!(dialog.filter_patterns(), &["*.txt"]);
        assert_eq!(dialog.filter_description(), "Text");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_leaves_the_rest_of_the_dialog_alone() {
        // `open_multiple_files` sets the path + multi-select BEFORE calling
        // apply_filter; the filter must not clobber either.
        let dialog = tfd::FileDialog::new("title")
            .with_path("/tmp/somewhere")
            .with_multiple_selection(true);
        let dialog = apply_filter(dialog, file_type_list(&["*.png"], "Images"));

        assert_eq!(dialog.path(), "/tmp/somewhere");
        assert!(dialog.multiple_selection());
        assert_eq!(dialog.filter_patterns(), &["*.png"]);
    }

    // ---------------------------------------------------------------------
    // FileTypeList / OptionFileTypeList container invariants
    // ---------------------------------------------------------------------

    #[test]
    fn string_vec_round_trips_through_into_library_owned_vec() {
        // This is the exact conversion `apply_filter` performs internally.
        let original: Vec<AzString> = vec![s("*.png"), s(""), s("*.🎨"), s("a\0b")];
        let round_tripped = StringVec::from_vec(original.clone()).into_library_owned_vec();
        assert_eq!(round_tripped, original);

        // ... and the empty case, which takes the null/zero-length branch.
        let empty = StringVec::from_vec(Vec::<AzString>::new()).into_library_owned_vec();
        assert!(empty.is_empty());
    }

    #[test]
    fn file_type_list_clone_is_equal_and_orders_reflexively() {
        use std::cmp::Ordering;

        let filter = file_type_list(&["*.png", "*.jpg"], "Images");
        let cloned = filter.clone();

        assert_eq!(cloned, filter);
        assert_eq!(filter.partial_cmp(&filter), Some(Ordering::Equal));
        assert_eq!(cloned.document_types.len(), 2);
        assert_eq!(cloned.document_descriptor.as_str(), "Images");
    }

    #[test]
    fn file_type_list_ordering_follows_the_descriptor_when_types_match() {
        use std::cmp::Ordering;

        let a = file_type_list(&["*.png"], "aaa");
        let b = file_type_list(&["*.png"], "bbb");
        assert_eq!(a.partial_cmp(&b), Some(Ordering::Less));
        assert_eq!(b.partial_cmp(&a), Some(Ordering::Greater));
        assert_ne!(a, b);
    }

    #[test]
    fn option_file_type_list_round_trips() {
        let filter = file_type_list(&["*.png"], "Images");

        let some = OptionFileTypeList::Some(filter.clone());
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(some.as_option(), Some(&filter));
        assert_eq!(some.clone().into_option(), Some(filter));

        let none = OptionFileTypeList::None;
        assert!(none.is_none());
        assert_eq!(none.as_option(), None);
        assert_eq!(OptionFileTypeList::default(), OptionFileTypeList::None);
    }

    // ---------------------------------------------------------------------
    // Modal entry points: signature guard only — calling these would block
    // ---------------------------------------------------------------------

    #[test]
    fn modal_entry_points_keep_their_ffi_signatures() {
        // Coercing to a fn pointer type-checks every exported signature WITHOUT
        // invoking it. api.json / the C bindings are generated from these exact
        // shapes, so an argument reorder or a changed return type must not slip
        // through unnoticed just because no test can safely call them.
        let _ok: fn(AzString, AzString, MsgBoxIcon) = MsgBox::ok;
        let _ok_cancel: fn(AzString, AzString, MsgBoxIcon, OkCancel) -> OkCancel =
            MsgBox::ok_cancel;
        let _yes_no: fn(AzString, AzString, MsgBoxIcon, YesNo) -> YesNo = MsgBox::yes_no;
        let _info: fn(AzString) = MsgBox::info;
        let _color: fn(AzString, OptionColorU) -> OptionColorU = ColorPickerDialog::open;
        let _open_file: fn(AzString, OptionString, OptionFileTypeList) -> OptionString =
            FileDialog::open_file;
        let _open_dir: fn(AzString, OptionString) -> OptionString = FileDialog::open_directory;
        let _open_many: fn(AzString, OptionString, OptionFileTypeList) -> OptionStringVec =
            FileDialog::open_multiple_files;
        let _save_file: fn(AzString, OptionString) -> OptionString = FileDialog::save_file;
        let _msg_box: fn(&str) = msg_box;
        let _open_file_async: fn(
            AzString,
            OptionString,
            OptionFileTypeList,
            bool,
        ) -> FilePickerHandle = FileDialog::open_file_async;
        let _save_file_async: fn(AzString, OptionString) -> FilePickerHandle =
            FileDialog::save_file_async;
        let _open_dir_async: fn(AzString, OptionString) -> FilePickerHandle =
            FileDialog::open_directory_async;
    }

    // ---------------------------------------------------------------------
    // Async picker handle: the part that never touches a native dialog
    // ---------------------------------------------------------------------

    /// A fresh handle is `Pending`; a backend's `set_status` is what every
    /// clone sees; a pre-answered handle is done on its first poll.
    #[test]
    fn picker_handle_is_shared_between_its_clones_and_answers_once_set() {
        let user_side = FilePickerHandle::new_pending();
        let backend_side = user_side.clone();
        assert_eq!(user_side.poll(), FilePickerStatus::Pending);
        assert!(!user_side.is_done());

        backend_side.set_status(FilePickerStatus::Selected(s("/tmp/a.txt")));
        drop(backend_side); // the backend drops its clone after answering
        assert!(user_side.is_done());
        assert_eq!(
            user_side.poll(),
            FilePickerStatus::Selected(s("/tmp/a.txt"))
        );

        // A flaky delegate that fires twice keeps the LATEST answer.
        user_side.set_status(FilePickerStatus::Cancelled);
        assert_eq!(user_side.poll(), FilePickerStatus::Cancelled);

        let answered = FilePickerHandle::with_status(FilePickerStatus::SelectedMultiple(
            StringVec::from_vec(vec![s("a"), s("b")]),
        ));
        assert!(
            answered.is_done(),
            "a pre-answered handle is done on its first poll"
        );

        // The backend answering AFTER the user dropped their handle must be
        // sound: the clone owns its own strong count. And the null `Default`
        // is an answered Error, never a handle that stays Pending.
        let user = FilePickerHandle::new_pending();
        let backend = user.clone();
        drop(user);
        backend.set_status(FilePickerStatus::Cancelled);
        assert_eq!(backend.poll(), FilePickerStatus::Cancelled);
        drop(backend);

        let null = FilePickerHandle::default();
        assert!(null.ptr.is_null() && !null.run_destructor);
        assert!(matches!(null.poll(), FilePickerStatus::Error(_)));
        assert!(null.is_done());
        assert!(null.clone().ptr.is_null());
        null.set_status(FilePickerStatus::Cancelled); // a no-op, not a crash
    }

    /// Without a registered backend nothing here may block, and the filter
    /// conversion hands the backends exactly the patterns, not the descriptor.
    #[test]
    fn filter_patterns_keep_the_types_and_drop_the_descriptor() {
        let list = file_type_list(&["*.png", "*.jpg"], "Images");
        let patterns = filter_patterns(OptionFileTypeList::Some(list));
        let v = patterns
            .into_option()
            .expect("patterns present")
            .into_library_owned_vec();
        let got: Vec<&str> = v.iter().map(AzString::as_str).collect();
        assert_eq!(got, vec!["*.png", "*.jpg"]);
        assert!(filter_patterns(OptionFileTypeList::None)
            .into_option()
            .is_none());
    }

    // ---------------------------------------------------------------------
    // android / iOS: the no-op branch is the one that CAN be executed safely
    // ---------------------------------------------------------------------

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_message_boxes_are_silent_no_ops() {
        MsgBox::ok(s("title"), s("message"), MsgBoxIcon::Error);
        MsgBox::info(s(""));
        msg_box("");
        msg_box("\0\u{1}🎨");
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_answer_dialogs_echo_the_default_back() {
        for default in [OkCancel::Ok, OkCancel::Cancel] {
            let answer = MsgBox::ok_cancel(s("t"), s("m"), MsgBoxIcon::Question, default);
            assert_eq!(answer, default);
        }
        for default in [YesNo::Yes, YesNo::No] {
            let answer = MsgBox::yes_no(s("t"), s("m"), MsgBoxIcon::Question, default);
            assert_eq!(answer, default);
        }
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_color_picker_echoes_the_default_back() {
        let default = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };
        let picked = ColorPickerDialog::open(s("t"), OptionColorU::Some(default));
        match picked.as_option() {
            Some(c) => {
                assert_eq!((c.r, c.g, c.b), (1, 2, 3));
                // NB: the mobile stub keeps the caller's alpha, while the desktop
                // path forces ColorU::ALPHA_OPAQUE. Pinned deliberately.
                assert_eq!(c.a, 4);
            }
            None => panic!("mobile stub must return the default it was given"),
        }
        assert!(ColorPickerDialog::open(s("t"), OptionColorU::None).is_none());
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_file_dialogs_report_cancellation() {
        assert!(
            FileDialog::open_file(s("t"), OptionString::None, OptionFileTypeList::None).is_none()
        );
        assert!(FileDialog::open_directory(s("t"), OptionString::None).is_none());
        assert!(FileDialog::save_file(s("t"), OptionString::Some(s("/tmp"))).is_none());
        assert!(FileDialog::open_multiple_files(
            s("t"),
            OptionString::None,
            OptionFileTypeList::None
        )
        .is_none());
    }
}
