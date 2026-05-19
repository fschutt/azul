//! Stub `dialogs` for Android / iOS — keeps the public type surface so
//! consumers (`azul-dll`'s `desktop::dialogs::*` re-exports) keep
//! compiling, but every entry point is a no-op that returns the
//! "safe / cancelled" default. Mobile platforms have no equivalent to
//! `tfd`'s modal dialogs from a pure-Rust crate; users should drive
//! their own in-app UI for file/color/message prompts.

use azul_css::{
    corety::OptionString,
    impl_option, impl_option_inner,
    props::basic::color::OptionColorU,
    AzString, OptionStringVec, StringVec,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct MsgBox {
    pub _reserved: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct FileDialog {
    pub _reserved: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct ColorPickerDialog {
    pub _reserved: u8,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum OkCancel {
    Ok,
    Cancel,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum YesNo {
    Yes,
    No,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum MsgBoxIcon {
    Info,
    Warning,
    Error,
    Question,
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

impl MsgBox {
    pub const fn new() -> Self { Self { _reserved: 0 } }

    pub fn ok(_title: AzString, _message: AzString, _icon: MsgBoxIcon) {
        // no-op on mobile
    }

    pub fn ok_cancel(
        _title: AzString,
        _message: AzString,
        _icon: MsgBoxIcon,
        default: OkCancel,
    ) -> OkCancel { default }

    pub fn yes_no(
        _title: AzString,
        _message: AzString,
        _icon: MsgBoxIcon,
        default: YesNo,
    ) -> YesNo { default }

    pub fn info(_content: AzString) { /* no-op */ }
}

impl ColorPickerDialog {
    pub const fn new() -> Self { Self { _reserved: 0 } }

    pub fn open(_title: AzString, default_value: OptionColorU) -> OptionColorU {
        default_value
    }
}

impl FileDialog {
    pub const fn new() -> Self { Self { _reserved: 0 } }

    pub fn open_file(
        _title: AzString,
        _default_path: OptionString,
        _filter_list: OptionFileTypeList,
    ) -> OptionString { OptionString::None }

    pub fn open_directory(_title: AzString, _default_path: OptionString) -> OptionString {
        OptionString::None
    }

    pub fn open_multiple_files(
        _title: AzString,
        _default_path: OptionString,
        _filter_list: OptionFileTypeList,
    ) -> OptionStringVec { OptionStringVec::None }

    pub fn save_file(_title: AzString, _default_path: OptionString) -> OptionString {
        OptionString::None
    }
}

pub fn msg_box(_content: &str) { /* no-op on mobile */ }
