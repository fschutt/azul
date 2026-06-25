//! File input button, same as `Button`, but triggers a
//! user-supplied path-change callback when clicked

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::Dom,
    refany::RefAny,
    resources::OptionImageRef,
};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::CssPropertyWithConditionsVec,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub container_style: CssPropertyWithConditionsVec,
    /// Style of the label
    pub label_style: CssPropertyWithConditionsVec,
    /// Style of the image
    pub image_style: CssPropertyWithConditionsVec,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Current state of the file input (selected path)
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileInputState {
    pub path: OptionString,
}

impl Default for FileInputState {
    fn default() -> Self {
        Self { path: None.into() }
    }
}

/// Callback type invoked when the file input path changes
pub type FileInputOnPathChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, FileInputState) -> Update;

impl_widget_callback!(
    FileInputOnPathChange,
    OptionFileInputOnPathChange,
    FileInputOnPathChangeCallback,
    FileInputOnPathChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        FileInputOnPathChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: FILE_INPUT_ON_PATH_CHANGE_INVOKER,
    invoker_ty:     AzFileInputOnPathChangeCallbackInvoker,
    thunk_fn:       az_file_input_on_path_change_callback_thunk,
    setter_fn:      AzApp_setFileInputOnPathChangeCallbackInvoker,
    from_handle_fn: AzFileInputOnPathChangeCallback_createFromHostHandle,
    extra_args:     [ state: FileInputState ],
}

impl FileInput {
    #[must_use] pub fn create(path: OptionString) -> Self {
        Self {
            file_input_state: FileInputStateWrapper {
                inner: FileInputState { path },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[inline]
    #[must_use]
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
    #[must_use] pub fn with_default_text(mut self, default_text: AzString) -> Self {
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
    #[must_use]
    pub fn with_on_path_change<I: Into<FileInputOnPathChangeCallback>>(
        mut self,
        refany: RefAny,
        callback: I,
    ) -> Self {
        self.set_on_path_change(refany, callback);
        self
    }

    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        // either show the default text or the file name
        // including the extension as the button label
        let button_label = match self.file_input_state.inner.path.as_ref() {
            Some(path) => std::path::Path::new(path.as_str())
                .file_name()
                .map_or_else(
                    || self.default_text.as_str().to_string(),
                    |s| s.to_string_lossy().to_string(),
                )
                .into(),
            None => self.default_text.clone(),
        };

        Button {
            label: button_label,
            image: self.image,
            button_type: crate::widgets::button::ButtonType::Default,
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
    let Some(mut fileinputstatewrapper) = refany.downcast_mut::<FileInputStateWrapper>() else {
        return Update::DoNothing;
    };
    let fileinputstatewrapper = &mut *fileinputstatewrapper;

    // `tfd` is desktop-only (target-gated in Cargo.toml to not(android|ios)); the
    // `extra` feature does nothing on mobile, so gate the dialog block by the same
    // target cfg to avoid referencing the unlinked `tfd` crate on iOS/Android.
    #[cfg(all(feature = "extra", not(any(target_os = "android", target_os = "ios"))))]
    {
        let mut dialog = tfd::FileDialog::new(fileinputstatewrapper.file_dialog_title.as_str());
        if let Some(dir) = fileinputstatewrapper.default_dir.as_ref() {
            dialog = dialog.with_path(dir.as_str());
        }
        let Some(selected_path) = dialog.open_file() else {
            return Update::DoNothing;
        };
        fileinputstatewrapper.inner.path = Some(selected_path.into()).into();
    }

    let inner = fileinputstatewrapper.inner.clone();
    let mut result = match fileinputstatewrapper.on_path_change.as_mut() {
        Some(FileInputOnPathChange { refany, callback }) => {
            (callback.cb)(refany.clone(), info, inner)
        }
        None => Update::RefreshDom,
    };

    result.max_self(Update::RefreshDom);

    result
}
