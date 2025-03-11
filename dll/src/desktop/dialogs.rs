#![allow(missing_copy_implementations)]

use core::ffi::c_void;

use azul_core::window::AzStringPair;
use azul_css::{impl_option, impl_option_inner, AzString, ColorU, StringVec};
use tfd::{DefaultColorValue, MessageBoxIcon};

/// Button dialog wrapper for reserved integration purposes
#[derive(Debug)]
pub struct MsgBox {
    /// reserved pointer (currently nullptr) for potential C extension
    pub _reserved: *mut c_void,
}

/// File dialog wrapper for reserved integration purposes
#[derive(Debug)]
pub struct FileDialog {
    /// reserved pointer (currently nullptr) for potential C extension
    pub _reserved: *mut c_void,
}

/// Color picker dialog wrapper for reserved integration purposes
#[derive(Debug)]
pub struct ColorPickerDialog {
    /// reserved pointer (currently nullptr) for potential C extension
    pub _reserved: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum OkCancel {
    Ok,
    Cancel,
}

impl From<tfd::OkCancel> for OkCancel {
    #[inline]
    fn from(e: tfd::OkCancel) -> Self {
        match e {
            tfd::OkCancel::Ok => OkCancel::Ok,
            tfd::OkCancel::Cancel => OkCancel::Cancel,
        }
    }
}

impl From<OkCancel> for tfd::OkCancel {
    #[inline]
    fn from(e: OkCancel) -> Self {
        match e {
            OkCancel::Ok => tfd::OkCancel::Ok,
            OkCancel::Cancel => tfd::OkCancel::Cancel,
        }
    }
}

/// "Ok / Cancel" MsgBox (title, message, icon, default)
pub fn msg_box_ok_cancel(
    title: &str,
    message: &str,
    icon: MessageBoxIcon,
    default: OkCancel,
) -> OkCancel {
    let msg_box = tfd::MessageBox::new(title, message)
        .with_icon(icon)
        .run_modal_ok_cancel(default.into());
    msg_box.into()
}

/// Yes or No result, returned from the `msg_box_yes_no` function
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum YesNo {
    Yes,
    No,
}

impl From<YesNo> for tfd::YesNo {
    #[inline]
    fn from(e: YesNo) -> Self {
        match e {
            YesNo::Yes => tfd::YesNo::Yes,
            YesNo::No => tfd::YesNo::No,
        }
    }
}

impl From<tfd::YesNo> for YesNo {
    #[inline]
    fn from(e: tfd::YesNo) -> Self {
        match e {
            tfd::YesNo::Yes => YesNo::Yes,
            tfd::YesNo::No => YesNo::No,
        }
    }
}

/// MsgBox icon to use in the `msg_box_*` functions
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum MsgBoxIcon {
    Info,
    Warning,
    Error,
    Question,
}

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

/// "Y/N" MsgBox (title, message, icon, default)
pub fn msg_box_yes_no(title: &str, message: &str, icon: MessageBoxIcon, default: YesNo) -> YesNo {
    let msg_box = tfd::MessageBox::new(title, message)
        .with_icon(icon)
        .run_modal_yes_no(default.into());
    msg_box.into()
}

/// "Ok" MsgBox (title, message, icon)
pub fn msg_box_ok(title: &str, message: &str, icon: MessageBoxIcon) {
    let mut msg = message.to_string();

    msg = msg.replace('\"', "");
    msg = msg.replace('\'', "");

    tfd::MessageBox::new(title, &msg)
        .with_icon(icon)
        .run_modal();
}

/// Wrapper around `message_box_ok` with the default title "Info" + an info icon.
pub fn msg_box(content: &str) {
    msg_box_ok("Info", content, MessageBoxIcon::Info);
}

/// Opens the default color picker dialog
pub fn color_picker_dialog(title: &str, default_value: Option<ColorU>) -> Option<ColorU> {
    let rgb = default_value.map_or([0, 0, 0], |c| [c.r, c.g, c.b]);

    let default_color = DefaultColorValue::RGB(rgb);
    let result = tfd::ColorChooser::new(title)
        .with_default_color(default_color)
        .run_modal()?;

    Some(ColorU {
        r: result.1[0],
        g: result.1[1],
        b: result.1[2],
        a: ColorU::ALPHA_OPAQUE,
    })
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

/// Open a single file, returns `None` if the user canceled the dialog.
///
/// Filters are the file extensions, i.e. `Some(&["doc", "docx"])` to only allow
/// "doc" and "docx" files
pub fn open_file_dialog(
    title: &str,
    default_path: Option<&str>,
    filter_list: Option<FileTypeList>,
) -> Option<AzString> {
    let mut dialog = tfd::FileDialog::new(title);

    if let Some(path) = default_path {
        dialog = dialog.with_path(path);
    }

    if let Some(filter) = filter_list {
        let v = filter.document_types.clone().into_library_owned_vec();

        let patterns: Vec<&str> = v.iter().map(|s| s.as_str()).collect();

        dialog = dialog.with_filter(&patterns, filter.document_descriptor.as_str());
    }

    dialog.open_file().map(|s| s.into())
}

/// Open a directory, returns `None` if the user canceled the dialog
pub fn open_directory_dialog(title: &str, default_path: Option<&str>) -> Option<AzString> {
    let mut dialog = tfd::FileDialog::new(title);

    if let Some(path) = default_path {
        dialog = dialog.with_path(path);
    }

    dialog.select_folder().map(|s| s.into())
}

/// Open multiple files at once, returns `None` if the user canceled the dialog,
/// otherwise returns the `Vec<String>` with the given file paths
///
/// Filters are the file extensions, i.e. `Some(&["doc", "docx"])` to only allow
/// "doc" and "docx" files
pub fn open_multiple_files_dialog(
    title: &str,
    default_path: Option<&str>,
    filter_list: Option<FileTypeList>,
) -> Option<StringVec> {
    let mut dialog = tfd::FileDialog::new(title).with_multiple_selection(true);

    if let Some(path) = default_path {
        dialog = dialog.with_path(path);
    }

    if let Some(filter) = filter_list {
        let v = filter.document_types.clone().into_library_owned_vec();

        let patterns: Vec<&str> = v.iter().map(|s| s.as_str()).collect();

        dialog = dialog.with_filter(&patterns, filter.document_descriptor.as_str());
    }

    dialog.open_files().map(|s| s.into())
}

/// Opens a save file dialog, returns `None` if the user canceled the dialog
pub fn save_file_dialog(title: &str, default_path: Option<&str>) -> Option<AzString> {
    let mut dialog = tfd::FileDialog::new(title);

    if let Some(path) = default_path {
        dialog = dialog.with_path(path);
    }

    dialog.save_file().map(|s| s.into())
}
