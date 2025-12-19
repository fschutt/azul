//! File input button, same as `Button`, but selects and
//! opens a file dialog instead

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, NodeDataInlineCssPropertyVec},
    refany::RefAny,
    resources::OptionImageRef,
};
use azul_css::{
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::button::{Button, ButtonOnClick, ButtonOnClickCallback},
};

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FileInput {
    /// State of the file input
    pub file_input_state: FileInputStateWrapper,
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
        let default_button = Button::create(AzString::from_const_str(""));
        Self {
            file_input_state: FileInputStateWrapper::default(),
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
    pub default_dir: OptionString,
}

impl Default for FileInputStateWrapper {
    fn default() -> Self {
        Self {
            inner: FileInputState::default(),
            on_path_change: None.into(),
            file_dialog_title: "Select File".into(),
            default_dir: None.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FileInputState {
    pub path: OptionString,
}

impl Default for FileInputState {
    fn default() -> Self {
        Self { path: None.into() }
    }
}

pub type FileInputOnPathChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, FileInputState) -> Update;

impl_widget_callback!(
    FileInputOnPathChange,
    OptionFileInputOnPathChange,
    FileInputOnPathChangeCallback,
    FileInputOnPathChangeCallbackType
);

impl FileInput {
    pub fn create(path: OptionString) -> Self {
        Self {
            file_input_state: FileInputStateWrapper {
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
        let mut s = Self::create(None.into());
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
    pub fn set_on_path_change<I: Into<FileInputOnPathChangeCallback>>(
        &mut self,
        refany: RefAny,
        callback: I,
    ) {
        self.file_input_state.on_path_change = Some(FileInputOnPathChange {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    #[inline]
    pub fn with_on_path_change<I: Into<FileInputOnPathChangeCallback>>(
        mut self,
        refany: RefAny,
        callback: I,
    ) -> Self {
        self.set_on_path_change(refany, callback);
        self
    }

    #[inline]
    pub fn dom(self) -> Dom {
        // either show the default text or the file name
        // including the extension as the button label
        let button_label = match self.file_input_state.inner.path.as_ref() {
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
                refany: RefAny::new(self.file_input_state),
                callback: ButtonOnClickCallback {
                    cb: fileinput_on_click,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            })
            .into(),
        }
        .dom()
    }
}

extern "C" fn fileinput_on_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let mut fileinputstatewrapper = match refany.downcast_mut::<FileInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    let fileinputstatewrapper = &mut *fileinputstatewrapper;

    // File dialog is not available in azul_layout
    // The user must provide their own file dialog callback via on_path_change
    // Just trigger the callback with the current state
    let inner = fileinputstatewrapper.inner.clone();
    let mut result = match fileinputstatewrapper.on_path_change.as_mut() {
        Some(FileInputOnPathChange { refany, callback }) => {
            (callback.cb)(refany.clone(), info.clone(), inner)
        }
        None => return Update::DoNothing,
    };

    result.max_self(Update::RefreshDom);

    result
}
