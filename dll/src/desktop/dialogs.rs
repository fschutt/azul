#![allow(missing_copy_implementations)]

use core::ffi::c_void;
use azul_css::{impl_option, impl_option_inner};
use azul_core::window::AzStringPair;
use azul_css::{AzString, ColorU, StringVec};
use tinyfiledialogs::{DefaultColorValue, MessageBoxIcon};

/// Ok or cancel result, returned from the `msg_box_ok_cancel` function
#[derive(Debug)]
pub struct MsgBox {
    /// reserved pointer (currently nullptr) for potential C extension
    pub _reserved: *mut c_void,
}

/// Ok or cancel result, returned from the `msg_box_ok_cancel` function
#[derive(Debug)]
pub struct FileDialog {
    /// reserved pointer (currently nullptr) for potential C extension
    pub _reserved: *mut c_void,
}

/// Ok or cancel result, returned from the `msg_box_ok_cancel` function
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

impl From<::tinyfiledialogs::OkCancel> for OkCancel {
    #[inline]
    fn from(e: ::tinyfiledialogs::OkCancel) -> OkCancel {
        match e {
            ::tinyfiledialogs::OkCancel::Ok => OkCancel::Ok,
            ::tinyfiledialogs::OkCancel::Cancel => OkCancel::Cancel,
        }
    }
}

impl From<OkCancel> for ::tinyfiledialogs::OkCancel {
    #[inline]
    fn from(e: OkCancel) -> ::tinyfiledialogs::OkCancel {
        match e {
            OkCancel::Ok => ::tinyfiledialogs::OkCancel::Ok,
            OkCancel::Cancel => ::tinyfiledialogs::OkCancel::Cancel,
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
    ::tinyfiledialogs::message_box_ok_cancel(title, message, icon, default.into()).into()
}

/// Yes or No result, returned from the `msg_box_yes_no` function
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum YesNo {
    Yes,
    No,
}

impl From<YesNo> for ::tinyfiledialogs::YesNo {
    #[inline]
    fn from(e: YesNo) -> ::tinyfiledialogs::YesNo {
        match e {
            YesNo::Yes => ::tinyfiledialogs::YesNo::Yes,
            YesNo::No => ::tinyfiledialogs::YesNo::No,
        }
    }
}

impl From<::tinyfiledialogs::YesNo> for YesNo {
    #[inline]
    fn from(e: ::tinyfiledialogs::YesNo) -> YesNo {
        match e {
            ::tinyfiledialogs::YesNo::Yes => YesNo::Yes,
            ::tinyfiledialogs::YesNo::No => YesNo::No,
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
    fn from(e: MsgBoxIcon) -> MessageBoxIcon {
        match e {
            MsgBoxIcon::Info => MessageBoxIcon::Info,
            MsgBoxIcon::Warning => MessageBoxIcon::Warning,
            MsgBoxIcon::Error => MessageBoxIcon::Error,
            MsgBoxIcon::Question => MessageBoxIcon::Question,
        }
    }
}

// Note: password_box, input_box and list_dialog do not work, so they're not included here.

/// "Y/N" MsgBox (title, message, icon, default)
pub fn msg_box_yes_no(title: &str, message: &str, icon: MessageBoxIcon, default: YesNo) -> YesNo {
    ::tinyfiledialogs::message_box_yes_no(title, message, icon, default.into()).into()
}

/// "Ok" MsgBox (title, message, icon)
pub fn msg_box_ok(title: &str, message: &str, icon: MessageBoxIcon) {
    let mut msg = message.to_string();

    #[cfg(target_os = "windows")]
    {
        // Windows does REALLY not like quotes in messages
        // otherwise the displayed message is just "INVALID MESSAGE WITH QUOTES"
        msg = msg.replace("\"", "");
        msg = msg.replace("\'", "");
    }

    #[cfg(target_os = "linux")]
    {
        msg = msg.replace("\"", "");
        msg = msg.replace("\'", "");
    }

    ::tinyfiledialogs::message_box_ok(title, &msg, icon)
}

/// Wrapper around `message_box_ok` with the default title "Info" + an info icon.
pub fn msg_box(content: &str) {
    msg_box_ok("Info", content, MessageBoxIcon::Info);
}

/// Opens the default color picker dialog
#[cfg(target_os = "windows")]
pub fn color_picker_dialog(title: &str, default_value: Option<ColorU>) -> Option<ColorU> {
    use winapi::{
        shared::minwindef::TRUE,
        um::{
            commdlg::{CC_ANYCOLOR, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW},
            wingdi::{GetBValue, GetGValue, GetRValue, RGB},
            winuser::GetForegroundWindow,
        },
    };

    let rgb = [
        default_value.map(|c| c.r).unwrap_or_default(),
        default_value.map(|c| c.g).unwrap_or_default(),
        default_value.map(|c| c.b).unwrap_or_default(),
    ];

    let mut crCustColors = [0_u32; 16];

    let mut cc = CHOOSECOLORW {
        lStructSize: core::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: unsafe { GetForegroundWindow() },
        hInstance: core::ptr::null_mut(),
        rgbResult: RGB(rgb[0], rgb[1], rgb[2]),
        lpCustColors: crCustColors.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR,
        lCustData: 0,
        lpfnHook: None,
        lpTemplateName: core::ptr::null_mut(),
    };

    let ret = unsafe { ChooseColorW(&mut cc) };

    if !ret == TRUE {
        None
    } else {
        Some(ColorU {
            r: GetRValue(cc.rgbResult),
            g: GetGValue(cc.rgbResult),
            b: GetBValue(cc.rgbResult),
            a: ColorU::ALPHA_OPAQUE,
        })
    }
}

#[cfg(not(target_os = "windows"))]
pub fn color_picker_dialog(title: &str, default_value: Option<ColorU>) -> Option<ColorU> {
    let rgb = [
        default_value.map(|c| c.r).unwrap_or_default(),
        default_value.map(|c| c.g).unwrap_or_default(),
        default_value.map(|c| c.b).unwrap_or_default(),
    ];

    let default = DefaultColorValue::RGB(&rgb);
    let result = ::tinyfiledialogs::color_chooser_dialog(title, default)?;
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
    let documents: Vec<AzString> = filter_list
        .as_ref()
        .map(|s| s.document_types.clone().into_library_owned_vec())
        .unwrap_or_default()
        .into();
    let documents: Vec<&str> = documents.iter().map(|s| s.as_str()).collect();
    let filter_list_ref = match filter_list.as_ref() {
        Some(s) => Some((documents.as_ref(), s.document_descriptor.as_str())),
        None => None,
    };
    let path = default_path.unwrap_or("");
    ::tinyfiledialogs::open_file_dialog(title, path, filter_list_ref).map(|s| s.into())
}

/// Open a directory, returns `None` if the user canceled the dialog
pub fn open_directory_dialog(title: &str, default_path: Option<&str>) -> Option<AzString> {
    ::tinyfiledialogs::select_folder_dialog(title, default_path.unwrap_or("")).map(|s| s.into())
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
    let documents: Vec<AzString> = filter_list
        .as_ref()
        .map(|s| s.document_types.clone().into_library_owned_vec())
        .unwrap_or_default()
        .into();
    let documents: Vec<&str> = documents.iter().map(|s| s.as_str()).collect();
    let filter_list_ref = match filter_list.as_ref() {
        Some(s) => Some((documents.as_ref(), s.document_descriptor.as_str())),
        None => None,
    };
    let path = default_path.unwrap_or("");
    ::tinyfiledialogs::open_file_dialog_multi(title, path, filter_list_ref).map(|s| s.into())
}

/// Opens a save file dialog, returns `None` if the user canceled the dialog
pub fn save_file_dialog(title: &str, default_path: Option<&str>) -> Option<AzString> {
    let path = default_path.unwrap_or("");
    ::tinyfiledialogs::save_file_dialog(title, path).map(|s| s.into())
}

// TODO (at least on Windows):
// - Find and replace dialog
// - Font picker dialog
// - Page setup dialog
// - Print dialog
// - Print property dialog
