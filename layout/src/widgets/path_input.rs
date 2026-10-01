//! Path input widget - a path field with a "Browse..." button beside it: an
//! installer's destination folder, a settings page's download folder, a
//! "Save to" line. The field takes typing; the button opens the platform's
//! folder (or file) picker through [`FileDialog`] and puts the answer into
//! the field. Either way the app hears the new path through ONE callback
//! ([`PathInput::on_change`]) and rebuilds with it.
//!
//! The widget owns no state: the app keeps the path and hands it back
//! ([`PathInput::create`]). A cancelled picker changes nothing. For
//! assistive technology the field is named by
//! [`PathInput::accessibility_name`] ("Destination folder") and the button
//! by its label; the button's description says what it picks.
//!
//! [`FileDialog`]: crate::desktop::dialogs::FileDialog
//!
//! Key types: [`PathInput`].

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomVec},
    refany::RefAny,
};
use azul_css::AzString;

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        button::{Button, ButtonOnClickCallbackType},
        dialog_kit::{self, DialogKitLook, FIXED_BASE, ROW_MIDDLE_BASE},
        shells::GROW_COLUMN_BASE,
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnTextInputCallbackType, TextInputState,
            TextInputValid,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The widget's class.
pub const PATH_INPUT_CLASS: &str = "__azul-native-path-input";
/// The class of the box around the field.
pub const PATH_FIELD_CLASS: &str = "__azul-native-path-input-field";
/// The class of the box around the Browse button.
pub const PATH_BROWSE_CLASS: &str = "__azul-native-path-input-browse";

/// Callback invoked with the new path: typed into the field, or picked.
pub type PathInputOnChangeCallbackType = extern "C" fn(RefAny, CallbackInfo, AzString) -> Update;
impl_widget_callback!(
    PathInputOnChange,
    OptionPathInputOnChange,
    PathInputOnChangeCallback,
    PathInputOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        PathInputOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: PATH_INPUT_ON_CHANGE_INVOKER,
    invoker_ty:     AzPathInputOnChangeCallbackInvoker,
    thunk_fn:       az_path_input_on_change_callback_thunk,
    setter_fn:      AzApp_setPathInputOnChangeCallbackInvoker,
    from_handle_fn: AzPathInputOnChangeCallback_createFromHostHandle,
    from_handle_byref_fn: AzPathInputOnChangeCallback_createFromHostHandleByref,
    extra_args:     [ path: AzString ],
}

/// A path field with a Browse button.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PathInput {
    /// The path shown in the field.
    pub path: AzString,
    /// The field's placeholder while it is empty.
    pub placeholder: AzString,
    /// The button's label ("Browse...").
    pub browse_label: AzString,
    /// The picker's title ("Choose the destination folder").
    pub dialog_title: AzString,
    /// What the field is called, for assistive technology.
    pub accessibility_name: AzString,
    /// Hears the new path.
    pub on_change: OptionPathInputOnChange,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// Whether Browse picks a folder (the default) or a file.
    pub directory: bool,
}

impl PathInput {
    /// A folder field holding `path`, with "Browse..." beside it.
    #[must_use]
    pub fn create(path: AzString) -> Self {
        Self {
            path,
            placeholder: AzString::from_const_str(""),
            browse_label: AzString::from_const_str("Browse..."),
            dialog_title: AzString::from_const_str("Choose a folder"),
            accessibility_name: AzString::from_const_str("Folder"),
            on_change: None.into(),
            theme: OptionUiTheme::None,
            directory: true,
        }
    }

    /// The field's placeholder.
    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.placeholder = placeholder;
    }

    /// [`Self::set_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    /// The button's label.
    pub fn set_browse_label(&mut self, label: AzString) {
        self.browse_label = label;
    }

    /// [`Self::set_browse_label`] for the builder chain.
    #[must_use]
    pub fn with_browse_label(mut self, label: AzString) -> Self {
        self.set_browse_label(label);
        self
    }

    /// The picker's title.
    pub fn set_dialog_title(&mut self, title: AzString) {
        self.dialog_title = title;
    }

    /// [`Self::set_dialog_title`] for the builder chain.
    #[must_use]
    pub fn with_dialog_title(mut self, title: AzString) -> Self {
        self.set_dialog_title(title);
        self
    }

    /// What the field is called, for assistive technology.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// Whether Browse picks a folder (`true`) or a file.
    pub const fn set_directory(&mut self, directory: bool) {
        self.directory = directory;
    }

    /// [`Self::set_directory`] for the builder chain.
    #[must_use]
    pub const fn with_directory(mut self, directory: bool) -> Self {
        self.set_directory(directory);
        self
    }

    /// The callback that hears the new path.
    pub fn set_on_change<C: Into<PathInputOnChangeCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_change = Some(PathInputOnChange {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_change`] for the builder chain.
    #[must_use]
    pub fn with_on_change<C: Into<PathInputOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// Pin the widget theme; unset, the widget follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty field and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// The widget's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = dialog_kit::look_for(self.theme);
        build(self, &look)
    }
}

impl Default for PathInput {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<PathInput> for Dom {
    fn from(p: PathInput) -> Self {
        p.dom()
    }
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

/// What the field and the button share.
struct PathRef {
    on_change: OptionPathInputOnChange,
    path: AzString,
    dialog_title: AzString,
    directory: bool,
}

/// Hands `path` to the app's callback.
fn report(data: &mut RefAny, info: CallbackInfo, path: AzString) -> Update {
    let Some(r) = data.downcast_ref::<PathRef>() else {
        return Update::DoNothing;
    };
    match r.on_change.as_ref() {
        Some(PathInputOnChange { callback, refany }) => callback.invoke(refany.clone(), info, path),
        None => Update::DoNothing,
    }
}

extern "C" fn on_typed(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let update = report(&mut data, info, AzString::from(state.get_text()));
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Browse: the click only ISSUES the picker request; the answer arrives in
/// [`on_picked`] as a fresh activation (FileInput's shape). Without the
/// `extra` feature there is no picker, and the click does nothing.
extern "C" fn on_browse(mut data: RefAny, info: CallbackInfo) -> Update {
    #[cfg(feature = "extra")]
    {
        use crate::desktop::dialogs::{FileDialog, OptionFileTypeList};

        let (title, start, directory) = {
            let Some(r) = data.downcast_ref::<PathRef>() else {
                return Update::DoNothing;
            };
            let start = if r.path.as_str().is_empty() {
                azul_css::OptionString::None
            } else {
                azul_css::OptionString::Some(r.path.clone())
            };
            (r.dialog_title.clone(), start, r.directory)
        };
        let _ = info;
        let resume = crate::callbacks::ResumeCallback::create(on_picked);
        let _request = if directory {
            FileDialog::open_directory(title, start, data.clone(), resume)
        } else {
            FileDialog::open_file(title, start, OptionFileTypeList::None, data.clone(), resume)
        };
        Update::DoNothing
    }
    #[cfg(not(feature = "extra"))]
    {
        let _ = (&mut data, info);
        Update::DoNothing
    }
}

/// The picker answered: a path goes to the app, a cancel changes nothing.
#[cfg(feature = "extra")]
extern "C" fn on_picked(mut data: RefAny, info: CallbackInfo, result: RefAny) -> Update {
    use crate::desktop::dialogs::FileOpenResult;

    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    report(&mut data, info, path.inner)
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

/// The widget's DOM in `look`: row [field box [TextInput], browse box
/// [Button]].
pub(crate) fn build(input: PathInput, look: &DialogKitLook) -> Dom {
    let PathInput {
        path,
        placeholder,
        browse_label,
        dialog_title,
        accessibility_name,
        on_change,
        theme,
        directory,
    } = input;
    let inner = dialog_kit::inner_theme(theme);
    let shared = RefAny::new(PathRef {
        on_change,
        path: path.clone(),
        dialog_title,
        directory,
    });

    let mut field = TextInput::create()
        .with_text(path)
        .with_placeholder(placeholder)
        .with_accessibility_name(accessibility_name)
        .with_on_text_input(shared.clone(), on_typed as TextInputOnTextInputCallbackType);
    if let Some(t) = inner {
        field = field.with_theme(t);
    }
    let mut browse =
        Button::create(browse_label).with_on_click(shared, on_browse as ButtonOnClickCallbackType);
    if let Some(t) = inner {
        browse = browse.with_theme(t);
    }
    let describes: &'static str = if directory {
        "Opens a folder picker"
    } else {
        "Opens a file picker"
    };
    let browse = browse
        .dom()
        .with_accessibility_assign(azul_core::a11y::AccessibilityInfo {
            description: Some(AzString::from_const_str(describes)).into(),
            ..Default::default()
        });

    let field_box = Dom::create_div()
        .with_ids_and_classes(dialog_kit::class(PATH_FIELD_CLASS))
        // A growing column: the field in it stretches to the box's width.
        .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &[]))
        .with_child(field.dom());
    let browse_box = Dom::create_div()
        .with_ids_and_classes(dialog_kit::class(PATH_BROWSE_CLASS))
        .with_css_props(dialog_kit::part(FIXED_BASE, &look.button))
        .with_child(browse);
    Dom::create_div()
        .with_ids_and_classes(dialog_kit::root_classes(PATH_INPUT_CLASS, look))
        .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &[]))
        .with_children(DomVec::from_vec(alloc::vec![field_box, browse_box]))
}

#[cfg(test)]
mod path_input_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::dom::{EventFilter, HoverEventFilter, NodeType};

    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, theme_checks as tc};

    type Log = Arc<Mutex<Vec<String>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, path: AzString) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(path.as_str().to_string());
        }
        Update::RefreshDom
    }

    fn input(log: &Log) -> PathInput {
        PathInput::create(AzString::from("/Applications/AzOffice"))
            .with_accessibility_name(AzString::from("Destination folder"))
            .with_on_change(
                RefAny::new(log.clone()),
                record as PathInputOnChangeCallbackType,
            )
    }

    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            out.push(s.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    #[test]
    fn the_widget_is_the_named_field_beside_the_browse_button() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = input(&log).with_theme(theme).dom();
            let kids = dom.children.as_ref();
            assert_eq!(kids.len(), 2, "{}: the field, the button", theme.name());
            assert!(tc::has_class(&kids[0], PATH_FIELD_CLASS));
            assert!(tc::has_class(&kids[1], PATH_BROWSE_CLASS));
            let mut button = Vec::new();
            texts(&kids[1], &mut button);
            assert_eq!(button, vec!["Browse..."], "{}", theme.name());
            let named = tc::nodes(&kids[0]).into_iter().any(|(_, n)| {
                n.root.get_accessibility_info().is_some_and(|i| {
                    i.accessibility_name.as_ref().map(|s| s.as_str()) == Some("Destination folder")
                })
            });
            assert!(named, "{}: the field is named", theme.name());
        }
    }

    #[test]
    fn browse_is_a_button_that_says_what_it_picks() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for (directory, says) in [
            (true, "Opens a folder picker"),
            (false, "Opens a file picker"),
        ] {
            let dom = input(&log)
                .with_directory(directory)
                .with_theme(UiTheme::Flat)
                .dom();
            let browse = &dom.children.as_ref()[1].children.as_ref()[0];
            let clicks = browse
                .root
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click));
            assert!(clicks, "Browse takes the click");
            let info = browse.root.get_accessibility_info().expect("a button");
            assert_eq!(
                info.description.as_ref().map(|d| d.as_str()),
                Some(says),
                "directory = {directory}"
            );
        }
    }

    #[test]
    fn an_unpinned_path_input_follows_the_app_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "path_input",
            || input(&log).dom(),
            |t: UiTheme| input(&log).with_theme(t).dom(),
        );
    }
}
