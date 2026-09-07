//! Native OS dialog wrappers (message boxes, file open/save, color picker).
//!
//! Desktop targets back this with the `tfd` (tiny-file-dialogs) crate; on
//! Android / iOS every method is a no-op that returns the "cancelled / safe
//! default" answer (there is no equivalent of `tfd` on those platforms from
//! a pure-Rust crate, and `tfd 0.1.0` does not cross-compile for them
//! anyway). The public type surface is identical on every target so
//! consumer code keeps compiling.

use azul_core::{refany::RefAny, task::RequestId};
use azul_css::{
    corety::OptionString,
    impl_option, impl_option_inner,
    props::basic::color::{ColorU, OptionColorU},
    AzString, OptionStringVec, StringVec, U8Vec,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tfd::{DefaultColorValue, MessageBoxIcon};

use crate::{
    callbacks::ResumeCallback,
    file::{FilePath, FilePathVec, OptionFilePath},
    request,
};

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

    /// Opens the system color picker and resumes `on_result` with a
    /// [`ColorPickResult`] (`color` is `None` if the user cancelled).
    ///
    /// The callback never runs re-entrantly inside the requesting activation:
    /// on desktop the picker is modal and the callback runs right after the
    /// current activation returns; on web it runs on a later task. Browsers
    /// only open the picker from a user gesture, and `<input type=color>` has
    /// no cancel event everywhere, so a blur without a change resolves as
    /// `None`.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn open(
        title: AzString,
        default_value: OptionColorU,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        match request::mock::take_color_pick() {
            request::mock::Answer::NotArmed => {}
            request::mock::Answer::Mocked(color) => {
                return request::complete(
                    data,
                    on_result,
                    ColorPickResult {
                        color: color.into(),
                    },
                );
            }
            request::mock::Answer::Unmocked => {
                return request::complete(
                    data,
                    on_result,
                    ColorPickResult {
                        color: OptionColorU::None,
                    },
                );
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let color = {
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
        };
        // No native color picker exists on mobile; the request resolves as
        // cancelled rather than never resolving.
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let color = {
            let _ = (title, default_value);
            OptionColorU::None
        };
        request::complete(data, on_result, ColorPickResult { color })
    }
}

// ============================================================================
// Resumable dialog results
// ============================================================================
//
// Every `FileDialog` / `ColorPickerDialog` request resumes its
// `ResumeCallback` with one of these structs, type-erased into a `RefAny`;
// the static `downcast(result)` accessor is the binding-portable way back
// to the typed value.

/// Result of [`FileDialog::open_file`] / [`FileDialog::open_directory`].
/// `path` is `None` if the user cancelled.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileOpenResult {
    pub path: OptionFilePath,
}

impl_option!(
    FileOpenResult,
    OptionFileOpenResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl FileOpenResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionFileOpenResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Result of [`FileDialog::open_multiple_files`]. `paths` is empty if the
/// user cancelled.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FileOpenMultiResult {
    pub paths: FilePathVec,
}

impl_option!(
    FileOpenMultiResult,
    OptionFileOpenMultiResult,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl FileOpenMultiResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionFileOpenMultiResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Result of [`ColorPickerDialog::open`]. `color` is `None` if the user
/// cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ColorPickResult {
    pub color: OptionColorU,
}

impl_option!(
    ColorPickResult,
    OptionColorPickResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl ColorPickResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionColorPickResult {
        result.downcast_ref::<Self>().map(|r| *r).into()
    }
}

/// What kind of write target a [`SaveTarget`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum SaveTargetKind {
    /// A real filesystem path (`as_path` is `Some`) - desktop and mobile.
    Path,
    /// A browser File-System-Access handle (Chromium); writes go to the
    /// user's chosen file, `as_path` is `None`.
    WebHandle,
    /// The portable browser fallback: `write_bytes` triggers a download of
    /// the bytes under the suggested name, `as_path` is `None`.
    Download,
}

/// An opaque write target obtained from [`FileDialog::save_file`].
///
/// Desktop: a real path. Web: a File-System-Access handle (Chromium) or a
/// `Download` sentinel (Firefox / Safari, which will not ship the handle
/// API), where [`SaveTarget::as_path`] returns `None`. Apps that only ever
/// export bytes should call [`FileDialog::save_bytes`] instead and never
/// touch this type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SaveTarget {
    pub kind: SaveTargetKind,
    pub path: OptionFilePath,
    /// Identifies the browser-side handle for `WebHandle` targets; `0`
    /// otherwise.
    pub handle_id: u64,
}

impl_option!(
    SaveTarget,
    OptionSaveTarget,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl SaveTarget {
    /// Writes `bytes` to the target. Fire-and-forget: `true` means the write
    /// was performed (desktop) or scheduled (web); durability is not implied.
    #[must_use]
    pub fn write_bytes(&self, bytes: U8Vec) -> bool {
        match self.kind {
            SaveTargetKind::Path => match self.path.as_ref() {
                Some(p) => crate::file::file_write(p.as_str(), bytes.as_ref()).is_ok(),
                None => false,
            },
            // Browser handles are serviced by the web host, never by native code.
            SaveTargetKind::WebHandle | SaveTargetKind::Download => false,
        }
    }

    /// The real path behind the target, or `None` on the browser fallbacks.
    #[must_use]
    pub fn as_path(&self) -> OptionFilePath {
        self.path.clone()
    }
}

/// Result of [`FileDialog::save_file`]. `target` is `None` if the user
/// cancelled.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SaveTargetResult {
    pub target: OptionSaveTarget,
}

impl_option!(
    SaveTargetResult,
    OptionSaveTargetResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl SaveTargetResult {
    /// Downcast the `result` `RefAny` delivered to a `ResumeCallback`.
    #[must_use]
    pub fn downcast(mut result: RefAny) -> OptionSaveTargetResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// Turns a picker status into the [`FileOpenResult`] the resumable API
/// delivers; `None` while the picker is still open. Multiple selections
/// collapse to the first path here (single-file request).
#[cfg(any(target_os = "android", target_os = "ios"))]
fn open_result_from_status(status: FilePickerStatus) -> Option<RefAny> {
    let path = match status {
        FilePickerStatus::Pending => return None,
        FilePickerStatus::Selected(p) => OptionFilePath::Some(FilePath::new(p)),
        FilePickerStatus::SelectedMultiple(v) => v
            .as_ref()
            .first()
            .cloned()
            .map(FilePath::new)
            .into(),
        FilePickerStatus::Cancelled | FilePickerStatus::Error(_) => OptionFilePath::None,
    };
    Some(RefAny::new(FileOpenResult { path }))
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

    /// Open a single file and resume `on_result` with a [`FileOpenResult`]
    /// (`path` is `None` if the user cancelled).
    ///
    /// Never blocks the calling activation in an observable way: on desktop
    /// the native modal dialog runs here and the callback runs right after
    /// the current activation returns; on mobile the OS picker is presented
    /// and the callback runs when its delegate answers; on web the callback
    /// always runs on a later task. `data` is handed back untouched.
    ///
    /// Browsers only open a picker from a user gesture: a request issued
    /// outside one (from a timer, for example) resolves with `path: None`.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn open_file(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        // Under an e2e run the picker is answered from the mock store (or
        // resolves as cancelled, loudly); a real dialog would hang the test.
        match request::mock::take_file_open("FileDialog::open_file") {
            request::mock::Answer::NotArmed => {}
            request::mock::Answer::Mocked(path) => {
                return request::complete(
                    data,
                    on_result,
                    FileOpenResult {
                        path: path.map(FilePath::new).into(),
                    },
                );
            }
            request::mock::Answer::Unmocked => {
                return request::complete(
                    data,
                    on_result,
                    FileOpenResult {
                        path: OptionFilePath::None,
                    },
                );
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            if let Some(filter) = filter_list.into_option() {
                dialog = apply_filter(dialog, filter);
            }
            let path = dialog
                .open_file()
                .map(|p| FilePath::new(AzString::from(p)))
                .into();
            request::complete(data, on_result, FileOpenResult { path })
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            match FILE_PICKER_BACKEND.get() {
                Some(backend) => {
                    let handle =
                        (backend.open_file)(title, default_path, filter_patterns(filter_list), false);
                    request::defer(
                        data,
                        on_result,
                        Box::new(move || open_result_from_status(handle.poll())),
                    )
                }
                // A shell that registered no picker: resolve as cancelled
                // instead of leaving the request open forever.
                None => request::complete(
                    data,
                    on_result,
                    FileOpenResult {
                        path: OptionFilePath::None,
                    },
                ),
            }
        }
    }

    /// Open a directory and resume `on_result` with a [`FileOpenResult`]
    /// whose `path` is the chosen directory (`None` if cancelled). Same
    /// contract as [`Self::open_file`].
    ///
    /// On web only Chromium has a directory picker; the portable fallback
    /// yields a read-only snapshot of the chosen tree, and `path` is the
    /// virtual root that snapshot is mounted at.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn open_directory(
        title: AzString,
        default_path: OptionString,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        match request::mock::take_file_open("FileDialog::open_directory") {
            request::mock::Answer::NotArmed => {}
            request::mock::Answer::Mocked(path) => {
                return request::complete(
                    data,
                    on_result,
                    FileOpenResult {
                        path: path.map(FilePath::new).into(),
                    },
                );
            }
            request::mock::Answer::Unmocked => {
                return request::complete(
                    data,
                    on_result,
                    FileOpenResult {
                        path: OptionFilePath::None,
                    },
                );
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            let path = dialog
                .select_folder()
                .map(|p| FilePath::new(AzString::from(p)))
                .into();
            request::complete(data, on_result, FileOpenResult { path })
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            match FILE_PICKER_BACKEND.get() {
                Some(backend) => {
                    let handle = (backend.open_directory)(title, default_path);
                    request::defer(
                        data,
                        on_result,
                        Box::new(move || open_result_from_status(handle.poll())),
                    )
                }
                None => request::complete(
                    data,
                    on_result,
                    FileOpenResult {
                        path: OptionFilePath::None,
                    },
                ),
            }
        }
    }

    /// Open multiple files and resume `on_result` with a
    /// [`FileOpenMultiResult`] (`paths` is empty if the user cancelled).
    /// Same contract as [`Self::open_file`].
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn open_multiple_files(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        match request::mock::take_file_open_multi() {
            request::mock::Answer::NotArmed => {}
            request::mock::Answer::Mocked(paths) => {
                let paths = paths.into_iter().map(FilePath::new).collect::<Vec<_>>();
                return request::complete(
                    data,
                    on_result,
                    FileOpenMultiResult {
                        paths: FilePathVec::from_vec(paths),
                    },
                );
            }
            request::mock::Answer::Unmocked => {
                return request::complete(
                    data,
                    on_result,
                    FileOpenMultiResult {
                        paths: FilePathVec::from_vec(Vec::new()),
                    },
                );
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str()).with_multiple_selection(true);
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            if let Some(filter) = filter_list.into_option() {
                dialog = apply_filter(dialog, filter);
            }
            let paths = dialog
                .open_files()
                .unwrap_or_default()
                .into_iter()
                .map(|p| FilePath::new(AzString::from(p)))
                .collect::<Vec<_>>();
            request::complete(
                data,
                on_result,
                FileOpenMultiResult {
                    paths: FilePathVec::from_vec(paths),
                },
            )
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            match FILE_PICKER_BACKEND.get() {
                Some(backend) => {
                    let handle =
                        (backend.open_file)(title, default_path, filter_patterns(filter_list), true);
                    request::defer(
                        data,
                        on_result,
                        Box::new(move || {
                            let paths = match handle.poll() {
                                FilePickerStatus::Pending => return None,
                                FilePickerStatus::Selected(p) => vec![FilePath::new(p)],
                                FilePickerStatus::SelectedMultiple(v) => v
                                    .as_ref()
                                    .iter()
                                    .cloned()
                                    .map(FilePath::new)
                                    .collect(),
                                FilePickerStatus::Cancelled | FilePickerStatus::Error(_) => {
                                    Vec::new()
                                }
                            };
                            Some(RefAny::new(FileOpenMultiResult {
                                paths: FilePathVec::from_vec(paths),
                            }))
                        }),
                    )
                }
                None => request::complete(
                    data,
                    on_result,
                    FileOpenMultiResult {
                        paths: FilePathVec::from_vec(Vec::new()),
                    },
                ),
            }
        }
    }

    /// Save-file dialog: resumes `on_result` with a [`SaveTargetResult`]
    /// whose `target` (`None` if cancelled) is *where to write*, not a
    /// string path - see [`SaveTarget`]. `suggested_name` is the file name
    /// the dialog proposes, not a path.
    ///
    /// Decision tree: an app that only ever exports a blob of bytes (a PDF,
    /// an image) should call [`Self::save_bytes`] and never see a target;
    /// use `save_file` when the app needs to write the same file again later
    /// (a document it keeps open). On web the real-path form exists only on
    /// Chromium; Firefox and Safari resolve with a `Download` target whose
    /// `as_path` is `None`.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn save_file(
        title: AzString,
        suggested_name: AzString,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        match request::mock::take_save_file() {
            request::mock::Answer::NotArmed => {}
            request::mock::Answer::Mocked(path) => {
                let target = path.map(|p| SaveTarget {
                    kind: SaveTargetKind::Path,
                    path: OptionFilePath::Some(FilePath::new(p)),
                    handle_id: 0,
                });
                return request::complete(
                    data,
                    on_result,
                    SaveTargetResult {
                        target: target.into(),
                    },
                );
            }
            request::mock::Answer::Unmocked => {
                return request::complete(
                    data,
                    on_result,
                    SaveTargetResult {
                        target: OptionSaveTarget::None,
                    },
                );
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if !suggested_name.as_str().is_empty() {
                dialog = dialog.with_path(suggested_name.as_str());
            }
            let target = dialog.save_file().map(|p| SaveTarget {
                kind: SaveTargetKind::Path,
                path: OptionFilePath::Some(FilePath::new(AzString::from(p))),
                handle_id: 0,
            });
            request::complete(
                data,
                on_result,
                SaveTargetResult {
                    target: target.into(),
                },
            )
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            match FILE_PICKER_BACKEND.get() {
                Some(backend) => {
                    let suggested = if suggested_name.as_str().is_empty() {
                        OptionString::None
                    } else {
                        OptionString::Some(suggested_name)
                    };
                    let handle = (backend.save_file)(title, suggested);
                    request::defer(
                        data,
                        on_result,
                        Box::new(move || {
                            let target = match handle.poll() {
                                FilePickerStatus::Pending => return None,
                                FilePickerStatus::Selected(p) => Some(SaveTarget {
                                    kind: SaveTargetKind::Path,
                                    path: OptionFilePath::Some(FilePath::new(p)),
                                    handle_id: 0,
                                }),
                                FilePickerStatus::SelectedMultiple(v) => {
                                    v.as_ref().first().cloned().map(|p| SaveTarget {
                                        kind: SaveTargetKind::Path,
                                        path: OptionFilePath::Some(FilePath::new(p)),
                                        handle_id: 0,
                                    })
                                }
                                FilePickerStatus::Cancelled | FilePickerStatus::Error(_) => None,
                            };
                            Some(RefAny::new(SaveTargetResult {
                                target: target.into(),
                            }))
                        }),
                    )
                }
                None => request::complete(
                    data,
                    on_result,
                    SaveTargetResult {
                        target: OptionSaveTarget::None,
                    },
                ),
            }
        }
    }

    /// Hand the user a file: `bytes` under `suggested_name` (a file name,
    /// not a path) with the given MIME type. Fire-and-forget; `true` means
    /// the export was performed (desktop: the user picked a location in the
    /// native save dialog and the file was written) or scheduled (web: a
    /// download was triggered). `false` means cancelled or failed.
    ///
    /// This is the portable "export a document" primitive: it works from
    /// any event-driven callback chain on every target, needs no write
    /// target and no path. Compose it with `CallbackInfo::take_screenshot`
    /// or `Pdf::save_to_bytes` for "save this as a file".
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn save_bytes(suggested_name: AzString, mime: AzString, bytes: U8Vec) -> bool {
        // An e2e run records the export instead of showing a dialog; the
        // scenario reads it back with `assert_saved_file`.
        if let Some(accepted) =
            request::mock::record_saved_file(&suggested_name, &mime, bytes.as_ref())
        {
            return accepted;
        }
        // The MIME type only matters to the browser (the download's
        // Content-Type); native save dialogs key off the name's extension.
        drop(mime);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new("Save");
            if !suggested_name.as_str().is_empty() {
                dialog = dialog.with_path(suggested_name.as_str());
            }
            match dialog.save_file() {
                Some(path) => crate::file::file_write(&path, bytes.as_ref()).is_ok(),
                None => false,
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            // No save dialog on mobile: the file lands in the app's
            // documents directory under the suggested name.
            let Some(dir) = FilePath::get_document_dir().or_else(FilePath::get_data_dir) else {
                return false;
            };
            let name = if suggested_name.as_str().is_empty() {
                "download"
            } else {
                suggested_name.as_str()
            };
            let target = dir.join_str(&AzString::from(name.to_string()));
            crate::file::file_write(target.as_str(), bytes.as_ref()).is_ok()
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
// the resumable `FileDialog::open_file` dispatches to it when one is present.

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
        // The pickers are requests: they take the app's context + resume
        // callback and answer through the runtime queue.
        let _color: fn(AzString, OptionColorU, RefAny, ResumeCallback) -> RequestId =
            ColorPickerDialog::open;
        let _open_file: fn(
            AzString,
            OptionString,
            OptionFileTypeList,
            RefAny,
            ResumeCallback,
        ) -> RequestId = FileDialog::open_file;
        let _open_dir: fn(AzString, OptionString, RefAny, ResumeCallback) -> RequestId =
            FileDialog::open_directory;
        let _open_many: fn(
            AzString,
            OptionString,
            OptionFileTypeList,
            RefAny,
            ResumeCallback,
        ) -> RequestId = FileDialog::open_multiple_files;
        let _save_file: fn(AzString, AzString, RefAny, ResumeCallback) -> RequestId =
            FileDialog::save_file;
        let _save_bytes: fn(AzString, AzString, U8Vec) -> bool = FileDialog::save_bytes;
        let _msg_box: fn(&str) = msg_box;
    }

    extern "C" fn resume_noop(
        _: RefAny,
        _: crate::callbacks::CallbackInfo,
        _: RefAny,
    ) -> azul_core::callbacks::Update {
        azul_core::callbacks::Update::DoNothing
    }

    /// Drains the runtime queue and returns the result struct the one request
    /// issued by the test resumed with.
    fn take_single_result() -> RefAny {
        let mut completed = crate::request::take_completed();
        assert_eq!(completed.len(), 1, "exactly one completion expected");
        completed.remove(0).result
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

    // No native color picker exists on mobile: the request resolves as
    // cancelled through the runtime queue instead of never resolving.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_color_picker_resolves_as_cancelled() {
        let _ = crate::request::take_completed();
        let default = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };
        let id = ColorPickerDialog::open(
            s("t"),
            OptionColorU::Some(default),
            RefAny::new(()),
            ResumeCallback::create(resume_noop),
        );
        assert!(id.is_valid());
        let picked = ColorPickResult::downcast(take_single_result())
            .into_option()
            .expect("a ColorPickResult");
        assert!(picked.color.is_none());
    }

    // A mobile build whose shell registered no picker backend resolves every
    // file dialog as cancelled - never a request that stays open forever.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_file_dialogs_without_a_backend_resolve_as_cancelled() {
        if has_file_picker_backend() {
            return;
        }
        let _ = crate::request::take_completed();
        let cb = ResumeCallback::create(resume_noop);

        FileDialog::open_file(
            s("t"),
            OptionString::None,
            OptionFileTypeList::None,
            RefAny::new(()),
            cb.clone(),
        );
        let open = FileOpenResult::downcast(take_single_result())
            .into_option()
            .expect("a FileOpenResult");
        assert!(open.path.is_none());

        FileDialog::open_directory(s("t"), OptionString::None, RefAny::new(()), cb.clone());
        let dir = FileOpenResult::downcast(take_single_result())
            .into_option()
            .expect("a FileOpenResult");
        assert!(dir.path.is_none());

        FileDialog::save_file(s("t"), s("doc.md"), RefAny::new(()), cb.clone());
        let save = SaveTargetResult::downcast(take_single_result())
            .into_option()
            .expect("a SaveTargetResult");
        assert!(save.target.is_none());

        FileDialog::open_multiple_files(
            s("t"),
            OptionString::None,
            OptionFileTypeList::None,
            RefAny::new(()),
            cb,
        );
        let many = FileOpenMultiResult::downcast(take_single_result())
            .into_option()
            .expect("a FileOpenMultiResult");
        assert!(many.paths.as_ref().is_empty());
    }
}
