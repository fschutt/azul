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
pub struct MsgBox {
    pub _reserved: u8,
}

/// Static-method namespace for `tfd`-backed file dialogs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct FileDialog {
    pub _reserved: u8,
}

/// Static-method namespace for the `tfd`-backed color picker.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
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
            tfd::OkCancel::Ok => OkCancel::Ok,
            tfd::OkCancel::Cancel => OkCancel::Cancel,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<OkCancel> for tfd::OkCancel {
    #[inline]
    fn from(e: OkCancel) -> Self {
        match e {
            OkCancel::Ok => tfd::OkCancel::Ok,
            OkCancel::Cancel => tfd::OkCancel::Cancel,
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
            YesNo::Yes => tfd::YesNo::Yes,
            YesNo::No => tfd::YesNo::No,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<tfd::YesNo> for YesNo {
    #[inline]
    fn from(e: tfd::YesNo) -> Self {
        match e {
            tfd::YesNo::Yes => YesNo::Yes,
            tfd::YesNo::No => YesNo::No,
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
            MsgBoxIcon::Info => MessageBoxIcon::Info,
            MsgBoxIcon::Warning => MessageBoxIcon::Warning,
            MsgBoxIcon::Error => MessageBoxIcon::Error,
            MsgBoxIcon::Question => MessageBoxIcon::Question,
        }
    }
}

impl MsgBox {
    /// Returns a zero-initialised namespace handle. The struct itself carries
    /// no state — instances exist only so the FFI layer can hang static
    /// methods off the type.
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// "Ok" message box — title, message, icon. Quotes are stripped from the
    /// message to work around `tfd` misinterpreting them as shell metacharacters
    /// on some platforms.
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
    pub fn yes_no(
        title: AzString,
        message: AzString,
        icon: MsgBoxIcon,
        default: YesNo,
    ) -> YesNo {
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

impl ColorPickerDialog {
    /// Returns a zero-initialised namespace handle. Static-only — the struct
    /// is just a hook for the FFI layer.
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Opens the default color picker dialog. Returns `None` if cancelled.
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct FileTypeList {
    pub document_types: StringVec,
    pub document_descriptor: AzString,
}

impl_option!(
    FileTypeList,
    OptionFileTypeList,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

/// Apply a [`FileTypeList`] filter to a `tfd::FileDialog`.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn apply_filter(mut dialog: tfd::FileDialog, filter: FileTypeList) -> tfd::FileDialog {
    let v = filter.document_types.clone().into_library_owned_vec();
    let patterns: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
    dialog = dialog.with_filter(&patterns, filter.document_descriptor.as_str());
    dialog
}

impl FileDialog {
    /// Returns a zero-initialised namespace handle. Static-only — the struct
    /// is just a hook for the FFI layer.
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Open a single file. Returns `None` if the user cancelled.
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
    pub fn open_multiple_files(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
    ) -> OptionStringVec {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog =
                tfd::FileDialog::new(title.as_str()).with_multiple_selection(true);
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

/// Convenience shim: show a default "Info" message box.
pub fn msg_box(content: &str) {
    MsgBox::info(AzString::from(content));
}
