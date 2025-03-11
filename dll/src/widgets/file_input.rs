//! File input button, same as `Button`, but selects and
//! opens a file dialog instead

use std::vec::Vec;

use azul_core::{
    app_resources::{ImageRef, OptionImageRef},
    callbacks::{CallbackInfo, RefAny, Update},
    dom::{
        Dom, IdOrClass,
        IdOrClass::Class,
        IdOrClassVec, NodeDataInlineCssProperty,
        NodeDataInlineCssProperty::{Active, Focus, Hover, Normal},
        NodeDataInlineCssPropertyVec, TabIndex,
    },
};
use azul_css::*;

use crate::{
    desktop::dialogs::OptionFileTypeList,
    widgets::button::{Button, ButtonOnClick, ButtonOnClickCallback},
};

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FileInput {
    /// State of the file input
    pub state: FileInputStateWrapper,
    /// Default text to display when no file has been selected
    /// (default = "Select File...")
    pub default_text: AzString,

    /// Optional image that is displayed next to the label
    pub image: OptionImageRef,
    /// Style for this button container
    pub container_style: NodeDataInlineCssPropertyVec,
    /// Style of the label
    pub label_style: NodeDataInlineCssPropertyVec,
    /// Style of the image
    pub image_style: NodeDataInlineCssPropertyVec,
}

impl Default for FileInput {
    fn default() -> Self {
        let default_button = Button::new(AzString::from_const_str(""));
        Self {
            state: FileInputStateWrapper::default(),
            default_text: "Select File...".into(),
            image: None.into(),
            container_style: default_button.container_style,
            label_style: default_button.label_style,
            image_style: default_button.image_style,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FileInputStateWrapper {
    pub inner: FileInputState,
    pub on_path_change: OptionFileInputOnPathChange,
    /// Title displayed in the file selection dialog
    pub file_dialog_title: AzString,
    /// Default directory of file input
    pub default_dir: OptionAzString,
    /// Selectable file types
    /// (default: None = user is able to select all file types)
    pub file_types: OptionFileTypeList,
}

impl Default for FileInputStateWrapper {
    fn default() -> Self {
        Self {
            inner: FileInputState::default(),
            on_path_change: None.into(),
            file_dialog_title: "Select File".into(),
            default_dir: None.into(),
            file_types: None.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FileInputState {
    pub path: OptionAzString,
}

impl Default for FileInputState {
    fn default() -> Self {
        Self { path: None.into() }
    }
}

pub type FileInputOnPathChangeCallbackType =
    extern "C" fn(&mut RefAny, &mut CallbackInfo, &FileInputState) -> Update;
impl_callback!(
    FileInputOnPathChange,
    OptionFileInputOnPathChange,
    FileInputOnPathChangeCallback,
    FileInputOnPathChangeCallbackType
);

impl FileInput {
    pub fn new(path: OptionAzString) -> Self {
        Self {
            state: FileInputStateWrapper {
                inner: FileInputState {
                    path,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(None.into());
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_default_text(&mut self, default_text: AzString) {
        self.default_text = default_text;
    }

    #[inline]
    pub fn with_default_text(mut self, default_text: AzString) -> Self {
        self.set_default_text(default_text);
        self
    }

    #[inline]
    pub fn set_on_path_change(
        &mut self,
        data: RefAny,
        callback: FileInputOnPathChangeCallbackType,
    ) {
        self.state.on_path_change = Some(FileInputOnPathChange {
            data,
            callback: FileInputOnPathChangeCallback { cb: callback },
        })
        .into();
    }

    #[inline]
    pub fn with_on_path_change(
        mut self,
        data: RefAny,
        callback: FileInputOnPathChangeCallbackType,
    ) -> Self {
        self.set_on_path_change(data, callback);
        self
    }

    #[inline]
    pub fn dom(mut self) -> Dom {
        // either show the default text or the file name
        // including the extension as the button label
        let button_label = match self.state.inner.path.as_ref() {
            Some(path) => std::path::Path::new(path.as_str())
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(self.default_text.as_str().to_string())
                .into(),
            None => self.default_text.clone(),
        };

        Button {
            label: button_label,
            image: self.image,
            container_style: self.container_style,
            label_style: self.label_style,
            image_style: self.image_style,
            on_click: Some(ButtonOnClick {
                data: RefAny::new(self.state),
                callback: ButtonOnClickCallback {
                    cb: fileinput_on_click,
                },
            })
            .into(),
        }
        .dom()
    }
}

extern "C" fn fileinput_on_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    use crate::desktop::dialogs::open_file_dialog;

    let mut fileinputstatewrapper = match data.downcast_mut::<FileInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    let mut fileinputstatewrapper = &mut *fileinputstatewrapper;

    // Open file select dialog
    let user_new_file_selected = match open_file_dialog(
        fileinputstatewrapper.file_dialog_title.as_str(),
        fileinputstatewrapper
            .default_dir
            .as_ref()
            .map(|s| s.as_str()),
        fileinputstatewrapper.file_types.clone().into_option(),
    ) {
        Some(s) => OptionAzString::Some(s),
        None => return Update::DoNothing,
    };

    fileinputstatewrapper.inner.path = user_new_file_selected;

    let mut result = match fileinputstatewrapper.on_path_change.as_mut() {
        Some(FileInputOnPathChange { data, callback }) => {
            (callback.cb)(data, info, &fileinputstatewrapper.inner)
        }
        None => return Update::DoNothing,
    };

    result.max_self(Update::RefreshDom);

    result
}
