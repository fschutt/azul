//! Bindings to platform-specific file dialogs using the `tinyfiledialogs` library

#![doc(inline)]

pub use tinyfiledialogs::DefaultColorValue;
pub use tinyfiledialogs::MessageBoxIcon;
pub use tinyfiledialogs::OkCancel;
pub use tinyfiledialogs::YesNo;

pub use tinyfiledialogs::color_chooser_dialog;
pub use tinyfiledialogs::input_box;
pub use tinyfiledialogs::list_dialog;
pub use tinyfiledialogs::message_box_ok;
pub use tinyfiledialogs::message_box_ok_cancel;
pub use tinyfiledialogs::message_box_yes_no;
pub use tinyfiledialogs::password_box;

// We don't use tinyfiledialogs for file dialogs
// because it doesn't handle Unicode correctly

/// Open a single file, returns `None` if the user canceled the dialog.
///
/// - `filter_list` may be `["doc", "docx", "jpg"]`
pub fn open_file_dialog(default_path: Option<&str>, filter_list: Option<&[&str]>)
-> Option<String>
{
    use nfd::{open_dialog, DialogType, Response};

    let filter_list = filter_list.map(|list| list.join(";"));
    let filter_list_2 = filter_list.as_ref().map(|x| &**x);

    match open_dialog(filter_list_2, default_path, DialogType::SingleFile).unwrap() {
        Response::Okay(file_path) => Some(file_path),
        _ => None,
    }
}

/// Open a directory, returns `None` if the user canceled the dialog
pub fn open_directory_dialog(default_path: Option<&str>)
-> Option<String>
{
    use nfd::{open_dialog, DialogType, Response};

    match open_dialog(None, default_path, DialogType::PickFolder).unwrap() {
        Response::Okay(file_path) => Some(file_path),
        _ => None,
    }
}

// Open multiple files at once, returns `None` if the user canceled the dialog,
// otherwise returns the `Vec<String>` with the given file paths
pub fn open_multiple_files_dialog(default_path: Option<&str>, filter_list: Option<&[&str]>)
-> Option<Vec<String>>
{
    use nfd::{open_dialog, DialogType, Response};

    let filter_list = filter_list.map(|list| list.join(";"));
    let filter_list_2 = filter_list.as_ref().map(|x| &**x);

    match open_dialog(filter_list_2, default_path, DialogType::MultipleFiles).unwrap() {
        Response::Okay(file_path) => Some(vec![file_path]),
        Response::OkayMultiple(paths) => Some(paths),
        _ => None,
    }
}

/// Opens a save file dialog, returns `None` if the user canceled the dialog
pub fn save_file_dialog(default_path: Option<&str>)
-> Option<String>
{
    use nfd::{open_dialog, DialogType, Response};

    match open_dialog(None, default_path, DialogType::SaveFile).unwrap() {
        Response::Okay(file_path) => Some(file_path),
        _ => None,
    }
}

pub fn message_box(content: &str) {
    message_box_ok("Info", content, MessageBoxIcon::Info);
}