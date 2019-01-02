//! Dialogs (open folder, open file), message boxes and native color pickers

pub use tinyfiledialogs::{DefaultColorValue, MessageBoxIcon};

/// Ok or cancel result, returned from the `msg_box_ok_cancel` function
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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
    ::tinyfiledialogs::message_box_ok(title, message, icon)
}

/// Wrapper around `message_box_ok` with the default title "Info" + an info icon.
///
/// Note: If you are too young to remember Visual Basics glorious `MsgBox`
/// then I pity you. Those were the days.
pub fn msg_box(content: &str) {
    msg_box_ok("Info", content, MessageBoxIcon::Info);
}

/// Color value (hex or rgb) to open the `color_chooser_dialog` with
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ColorValue<'a> {
    Hex(&'a str),
    RGB(&'a [u8; 3]),
}

/// Default color in the color picker
const DEFAULT_COLOR: [u8; 3] = [0, 0, 0];

impl<'a> Default for ColorValue<'a> {
    fn default() -> Self {
        ColorValue::RGB(&DEFAULT_COLOR)
    }
}

impl<'a> Into<DefaultColorValue<'a>> for ColorValue<'a> {
    fn into(self) -> DefaultColorValue<'a> {
        match self {
            ColorValue::Hex(s) => DefaultColorValue::Hex(s),
            ColorValue::RGB(r) => DefaultColorValue::RGB(r),
        }
    }
}

/// Opens the default color picker dialog
pub fn color_picker_dialog(
    title: &str,
    default_value: Option<ColorValue>,
) -> Option<(String, [u8; 3])> {
    let default = default_value.unwrap_or_default().into();
    ::tinyfiledialogs::color_chooser_dialog(title, default)
}

// The difference between tinyfiledialogs and nfd is that nfd links
// to a specific dialog at compile time, while tinyfiledialogs selects
// the dialog at runtime from a set of specific dialogs (i.e. Mate, KDE,
// dolphin, whatever). This (a) doesn't force the library user to choose
// a specific dialog, (b) won't look non-native (GTK3 on a KDE env can
// look jarring) and (c) doesn't require the user to install extra libraries
//
// The only reason why we don't use tinyfiledialogs everywhere is because
// it doesn't handle unicode correctly (so if the user has öüä in his username,
// it won't return a correct file path). However, tinyfiledialogs **does**
// handle Unicode correctly on Linux. So the solution is to use tinyfiledialogs
// on Linux (because of the dependency issue) and nfd everywhere else (because
// of better Unicode)

/// Open a single file, returns `None` if the user canceled the dialog.
///
/// Filters are the file extensions, i.e. `Some(&["doc", "docx"])` to only allow
/// "doc" and "docx" files
#[cfg(not(target_os = "linux"))]
pub fn open_file_dialog(
    default_path: Option<&str>,
    filter_list: Option<&[&str]>,
) -> Option<String> {
    use nfd::{open_dialog, DialogType, Response};

    let filter_list = filter_list.map(|list| list.join(";"));
    let filter_list_2 = filter_list.as_ref().map(|x| &**x);

    match open_dialog(filter_list_2, default_path, DialogType::SingleFile).unwrap() {
        Response::Okay(file_path) => Some(file_path),
        _ => None,
    }
}

/// Open a single file, returns `None` if the user canceled the dialog.
///
/// Filters are the file extensions, i.e. `Some(&["doc", "docx"])` to only allow
/// "doc" and "docx" files
#[cfg(target_os = "linux")]
pub fn open_file_dialog(
    default_path: Option<&str>,
    filter_list: Option<&[&str]>,
) -> Option<String> {
    let filter_list = filter_list.map(|f| (f, ""));
    let path = default_path.unwrap_or("");
    ::tinyfiledialogs::open_file_dialog("Open File", path, filter_list)
}

/// Open a directory, returns `None` if the user canceled the dialog
#[cfg(not(target_os = "linux"))]
pub fn open_directory_dialog(default_path: Option<&str>) -> Option<String> {
    use nfd::{open_dialog, DialogType, Response};

    match open_dialog(None, default_path, DialogType::PickFolder).unwrap() {
        Response::Okay(file_path) => Some(file_path),
        _ => None,
    }
}

/// Open a directory, returns `None` if the user canceled the dialog
#[cfg(target_os = "linux")]
pub fn open_directory_dialog(default_path: Option<&str>) -> Option<String> {
    ::tinyfiledialogs::select_folder_dialog("Open Filder", default_path.unwrap_or(""))
}

/// Open multiple files at once, returns `None` if the user canceled the dialog,
/// otherwise returns the `Vec<String>` with the given file paths
///
/// Filters are the file extensions, i.e. `Some(&["doc", "docx"])` to only allow
/// "doc" and "docx" files
#[cfg(not(target_os = "linux"))]
pub fn open_multiple_files_dialog(
    default_path: Option<&str>,
    filter_list: Option<&[&str]>,
) -> Option<Vec<String>> {
    use nfd::{open_dialog, DialogType, Response};

    let filter_list = filter_list.map(|list| list.join(";"));
    let filter_list_2 = filter_list.as_ref().map(|x| &**x);

    match open_dialog(filter_list_2, default_path, DialogType::MultipleFiles).unwrap() {
        Response::Okay(file_path) => Some(vec![file_path]),
        Response::OkayMultiple(paths) => Some(paths),
        _ => None,
    }
}

/// Open multiple files at once, returns `None` if the user canceled the dialog,
/// otherwise returns the `Vec<String>` with the given file paths
///
/// Filters are the file extensions, i.e. `Some(&["doc", "docx"])` to only allow
/// "doc" and "docx" files
#[cfg(target_os = "linux")]
pub fn open_multiple_files_dialog(
    default_path: Option<&str>,
    filter_list: Option<&[&str]>,
) -> Option<Vec<String>> {
    let filter_list = filter_list.map(|f| (f, ""));
    let path = default_path.unwrap_or("");
    ::tinyfiledialogs::open_file_dialog_multi("Open Folder", path, filter_list)
}

/// Opens a save file dialog, returns `None` if the user canceled the dialog
#[cfg(not(target_os = "linux"))]
pub fn save_file_dialog(default_path: Option<&str>) -> Option<String> {
    use nfd::{open_dialog, DialogType, Response};

    match open_dialog(None, default_path, DialogType::SaveFile).unwrap() {
        Response::Okay(file_path) => Some(file_path),
        _ => None,
    }
}

/// Opens a save file dialog, returns `None` if the user canceled the dialog
#[cfg(target_os = "linux")]
pub fn save_file_dialog(default_path: Option<&str>) -> Option<String> {
    let path = default_path.unwrap_or("");
    ::tinyfiledialogs::save_file_dialog("Save File", path)
}

// TODO (at least on Windows):
// - Find and replace dialog
// - Font picker dialog
// - Page setup dialog
// - Print dialog
// - Print property dialog
