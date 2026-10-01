//! Standard dialogs - the BODIES of the dialogs every desktop app shows:
//!
//! | dialog                  | what it shows                                                       |
//! |-------------------------|---------------------------------------------------------------------|
//! | [`MessageBox`]          | an info / warning / error / question glyph, a title, the text, the buttons, "Don't ask again" |
//! | [`AboutDialog`]         | the icon, the name, the version, the copyright, the credits and their licenses |
//! | [`ProgressDialog`]      | the status, a bar (or a spinner when the end is unknown), Cancel    |
//! | [`LoginDialog`]         | user name, password, "Remember me", Sign in / Cancel                |
//! | [`FindReplaceDialog`]   | find, replace, match case / whole word, the result, the buttons     |
//!
//! A body is a `Dom`: an app shows it in a [`Modal`](crate::widgets::modal::Modal)
//! or a [`Dialog`](crate::widgets::dialog::Dialog) (which give the window,
//! the title bar, Escape and the focus handling), a Backstage page or a
//! window of its own. They are not the OS's message boxes
//! (`desktop::dialogs::MsgBox`, which cannot carry "Don't ask again" or
//! follow the app theme).
//!
//! The dialogs own nothing: the app keeps every value and hears every
//! request through ONE callback type ([`StandardDialogOnEventCallbackType`],
//! a [`StandardDialogEvent`] saying what and which). The login dialog
//! STORES nothing - the app takes the password from the event, hands it to
//! the keyring or the server, and clears it.
//!
//! Key types: the five dialogs, [`StandardDialogEvent`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::Update,
    dom::{Dom, DomVec, TabIndex},
    refany::RefAny,
    window::StringPairVec,
};
use azul_css::{AzString, StringVec};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        button::{ButtonOnClickCallbackType, ButtonType},
        check_box::{CheckBoxOnToggleCallbackType, CheckBoxState},
        dialog_kit::{
            self, DialogKitLook, BUTTON_BOX_BASE, BUTTON_BOX_CLASS, BUTTON_ROW_BASE, FIXED_BASE,
            HELD_CLASS, ROW_MIDDLE_BASE, ROW_TOP_BASE, SCROLL_BOX_BASE, SPACER_BASE,
        },
        shells::{COLUMN_BASE, GROW_COLUMN_BASE, GROW_LABEL_BASE},
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The message box's class.
pub const MESSAGE_BOX_CLASS: &str = "__azul-native-message-box";
/// The about dialog's class.
pub const ABOUT_CLASS: &str = "__azul-native-about-dialog";
/// The class of the credits list.
pub const ABOUT_CREDITS_CLASS: &str = "__azul-native-about-dialog-credits";
/// The progress dialog's class.
pub const PROGRESS_DIALOG_CLASS: &str = "__azul-native-progress-dialog";
/// The login dialog's class.
pub const LOGIN_CLASS: &str = "__azul-native-login-dialog";
/// The find / replace dialog's class.
pub const FIND_REPLACE_CLASS: &str = "__azul-native-find-replace-dialog";
/// The class of a standard dialog's button row.
pub const DIALOG_BUTTONS_CLASS: &str = "__azul-native-standard-dialog-buttons";
/// The class of a standard dialog's checkbox rows.
pub const DIALOG_CHECK_CLASS: &str = "__azul-native-standard-dialog-check";

// ---------------------------------------------------------------------------
// The event
// ---------------------------------------------------------------------------

/// What the user asked of a standard dialog.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StandardDialogEventKind {
    /// Button `index` of a message box (or the about box's OK).
    Button,
    /// Cancel / Close.
    Cancel,
    /// "Don't ask again" changed: `checked`.
    DontAskAgain,
    /// Field `index` reads `text` (login: 0 user, 1 password; find: 0 find,
    /// 1 replace).
    FieldChanged,
    /// Option `index` changed: `checked` (login: 0 remember; find: 0 match
    /// case, 1 whole word).
    OptionToggled,
    /// Sign in (the button, or Enter in a field).
    Submit,
    /// Find the next match (the button, or Enter in the find field).
    FindNext,
    /// Find the previous match.
    FindPrevious,
    /// Replace this match.
    Replace,
    /// Replace every match.
    ReplaceAll,
}

/// One request from a standard dialog.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardDialogEvent {
    /// The field's text (`FieldChanged`), else empty.
    pub text: AzString,
    /// The button, field or option, else 0.
    pub index: usize,
    /// What was asked.
    pub kind: StandardDialogEventKind,
    /// The new value of a checkbox (`DontAskAgain`, `OptionToggled`).
    pub checked: bool,
}

impl StandardDialogEvent {
    /// An event of `kind` at `index`.
    #[must_use]
    pub fn create(kind: StandardDialogEventKind, index: usize, checked: bool) -> Self {
        Self {
            text: AzString::from_const_str(""),
            index,
            kind,
            checked,
        }
    }
}

/// Callback invoked for a request from a standard dialog.
pub type StandardDialogOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, StandardDialogEvent) -> Update;
impl_widget_callback!(
    StandardDialogOnEvent,
    OptionStandardDialogOnEvent,
    StandardDialogOnEventCallback,
    StandardDialogOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        StandardDialogOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: STANDARD_DIALOG_ON_EVENT_INVOKER,
    invoker_ty:     AzStandardDialogOnEventCallbackInvoker,
    thunk_fn:       az_standard_dialog_on_event_callback_thunk,
    setter_fn:      AzApp_setStandardDialogOnEventCallbackInvoker,
    from_handle_fn: AzStandardDialogOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzStandardDialogOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: StandardDialogEvent ],
}

fn on_event_of<C: Into<StandardDialogOnEventCallback>>(
    data: RefAny,
    callback: C,
) -> OptionStandardDialogOnEvent {
    Some(StandardDialogOnEvent {
        refany: data,
        callback: callback.into(),
    })
    .into()
}

/// The theme pin, `swap_with_default`, `dom`, `Default` and `From` every
/// dialog shares.
macro_rules! dialog_theme_and_dom {
    ($dialog:ident, $build:ident, $default:expr) => {
        impl $dialog {
            /// Pin the widget theme; unset, the dialog follows the app theme.
            pub const fn set_theme(&mut self, theme: UiTheme) {
                self.theme = OptionUiTheme::Some(theme);
            }

            /// [`Self::set_theme`] for the builder chain.
            #[must_use]
            pub const fn with_theme(mut self, theme: UiTheme) -> Self {
                self.set_theme(theme);
                self
            }

            /// The callback that hears every request.
            pub fn set_on_event<C: Into<StandardDialogOnEventCallback>>(
                &mut self,
                data: RefAny,
                callback: C,
            ) {
                self.on_event = on_event_of(data, callback);
            }

            /// [`Self::set_on_event`] for the builder chain.
            #[must_use]
            pub fn with_on_event<C: Into<StandardDialogOnEventCallback>>(
                mut self,
                data: RefAny,
                callback: C,
            ) -> Self {
                self.set_on_event(data, callback);
                self
            }

            /// Replaces `self` with an empty dialog and returns the original.
            #[must_use]
            pub fn swap_with_default(&mut self) -> Self {
                let mut s = $default;
                core::mem::swap(&mut s, self);
                s
            }

            /// The dialog's body.
            #[must_use]
            pub fn dom(self) -> Dom {
                let look = dialog_kit::look_for(self.theme);
                $build(self, &look)
            }
        }

        impl Default for $dialog {
            fn default() -> Self {
                $default
            }
        }

        impl From<$dialog> for Dom {
            fn from(d: $dialog) -> Self {
                d.dom()
            }
        }
    };
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

/// What a button, a checkbox row or a field reports: the callback, the
/// report and (a checkbox) its current value.
struct Report {
    on_event: OptionStandardDialogOnEvent,
    kind: StandardDialogEventKind,
    index: usize,
    checked: bool,
    /// What Enter in a field asks (`Submit`, `FindNext`), if anything.
    enter: Option<StandardDialogEventKind>,
}

fn emit(
    on_event: &OptionStandardDialogOnEvent,
    info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    match on_event.as_ref() {
        Some(StandardDialogOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, event)
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_button(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(r) = data.downcast_ref::<Report>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        StandardDialogEvent::create(r.kind, r.index, r.checked),
    )
}

extern "C" fn on_check(mut data: RefAny, info: CallbackInfo, state: CheckBoxState) -> Update {
    let Some(r) = data.downcast_ref::<Report>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        StandardDialogEvent::create(r.kind, r.index, state.checked),
    )
}

extern "C" fn on_check_label(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(r) = data.downcast_ref::<Report>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        StandardDialogEvent::create(r.kind, r.index, !r.checked),
    )
}

// ---------------------------------------------------------------------------
// Shared parts of the build
// ---------------------------------------------------------------------------

fn report(
    on_event: &OptionStandardDialogOnEvent,
    kind: StandardDialogEventKind,
    index: usize,
    checked: bool,
) -> RefAny {
    RefAny::new(Report {
        on_event: on_event.clone(),
        kind,
        index,
        checked,
        enter: None,
    })
}

/// A button of the row: `primary` or not, reporting `kind` at `index`, or
/// inert when `enabled` is unset.
#[allow(clippy::too_many_arguments)]
fn button(
    label: &AzString,
    primary: bool,
    kind: StandardDialogEventKind,
    index: usize,
    enabled: bool,
    on_event: &OptionStandardDialogOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    dialog_kit::row_button(
        label.clone(),
        if primary {
            ButtonType::Primary
        } else {
            ButtonType::Default
        },
        enabled.then(|| {
            (
                report(on_event, kind, index, false),
                on_button as ButtonOnClickCallbackType,
            )
        }),
        None,
        theme,
        (BUTTON_BOX_CLASS, HELD_CLASS),
        BUTTON_BOX_BASE,
        (&look.button, &look.held),
    )
}

/// A checkbox row reporting `kind` at `index`.
#[allow(clippy::too_many_arguments)]
fn check(
    label: &AzString,
    checked: bool,
    kind: StandardDialogEventKind,
    index: usize,
    on_event: &OptionStandardDialogOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    dialog_kit::check_row(
        label,
        checked,
        (
            report(on_event, kind, index, checked),
            on_check as CheckBoxOnToggleCallbackType,
            on_check_label as ButtonOnClickCallbackType,
        ),
        DIALOG_CHECK_CLASS,
        theme,
        look,
    )
}

/// A dialog's body: [content (the dialog's padding), button row [spacer,
/// buttons]], in the kit's page face, named for assistive technology.
fn body(
    class: &'static str,
    a11y: AccessibilityInfo,
    content: Vec<Dom>,
    buttons: Vec<Dom>,
    look: &DialogKitLook,
) -> Dom {
    let mut row: Vec<Dom> =
        alloc::vec![Dom::create_div().with_css_props(dialog_kit::part(SPACER_BASE, &[]))];
    row.extend(buttons);
    Dom::create_div()
        .with_ids_and_classes(dialog_kit::root_classes(class, look))
        .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &look.page))
        .with_accessibility_info(a11y)
        .with_children(DomVec::from_vec(alloc::vec![
            Dom::create_div()
                .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &look.dialog))
                .with_children(DomVec::from_vec(content)),
            Dom::create_div()
                .with_ids_and_classes(dialog_kit::class(DIALOG_BUTTONS_CLASS))
                .with_css_props(dialog_kit::part(BUTTON_ROW_BASE, &look.buttons))
                .with_children(DomVec::from_vec(row)),
        ]))
}

/// The widget's own text, one block per paragraph (`\n\n` apart).
fn paragraphs(
    text: &AzString,
    skin: &[azul_css::dynamic_selector::CssPropertyWithConditions],
    look: &DialogKitLook,
) -> Vec<Dom> {
    let mut block = skin.to_vec();
    block.extend_from_slice(&look.block);
    text.as_str()
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| dialog_kit::line(AzString::from(p.trim()), &[], &block))
        .collect()
}

// ---------------------------------------------------------------------------
// MessageBox
// ---------------------------------------------------------------------------

/// What a message box tells.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum MessageBoxKind {
    /// Information: the blue "i".
    #[default]
    Info,
    /// A warning: the yellow triangle.
    Warning,
    /// An error: the red circle.
    Error,
    /// A question: the accent "?".
    Question,
}

impl MessageBoxKind {
    /// The glyph's icon name.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Question => "help",
        }
    }
}

/// A message box: a glyph, a title, the text, a detail line, the buttons
/// and an optional "Don't ask again".
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MessageBox {
    /// The title ("Save changes to Report.docx?").
    pub title: AzString,
    /// The text; paragraphs are `\n\n` apart.
    pub text: AzString,
    /// A line in the secondary ink under the text, or empty.
    pub detail: AzString,
    /// The buttons, left to right ("Save", "Don't Save", "Cancel").
    pub buttons: StringVec,
    /// The "Don't ask again" label, or empty for no checkbox.
    pub dont_ask_label: AzString,
    /// Hears `Button(index)` and `DontAskAgain`.
    pub on_event: OptionStandardDialogOnEvent,
    /// The primary button (the default action).
    pub default_button: usize,
    /// What it tells (the glyph).
    pub kind: MessageBoxKind,
    /// The widget theme this dialog is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
    /// Whether "Don't ask again" is ticked.
    pub dont_ask: bool,
}

impl MessageBox {
    /// A message box of `kind` titled `title` saying `text`, with one OK
    /// button.
    #[must_use]
    pub fn create(kind: MessageBoxKind, title: AzString, text: AzString) -> Self {
        Self {
            title,
            text,
            detail: AzString::from_const_str(""),
            buttons: StringVec::from_vec(alloc::vec![AzString::from_const_str("OK")]),
            dont_ask_label: AzString::from_const_str(""),
            on_event: None.into(),
            default_button: 0,
            kind,
            theme: OptionUiTheme::None,
            dont_ask: false,
        }
    }

    /// The buttons and which one is the default.
    pub fn set_buttons(&mut self, buttons: StringVec, default_button: usize) {
        self.buttons = buttons;
        self.default_button = default_button;
    }

    /// [`Self::set_buttons`] for the builder chain.
    #[must_use]
    pub fn with_buttons(mut self, buttons: StringVec, default_button: usize) -> Self {
        self.set_buttons(buttons, default_button);
        self
    }

    /// The line under the text.
    pub fn set_detail(&mut self, detail: AzString) {
        self.detail = detail;
    }

    /// [`Self::set_detail`] for the builder chain.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.set_detail(detail);
        self
    }

    /// "Don't ask again": its label (empty: none) and whether it is ticked.
    pub fn set_dont_ask(&mut self, label: AzString, checked: bool) {
        self.dont_ask_label = label;
        self.dont_ask = checked;
    }

    /// [`Self::set_dont_ask`] for the builder chain.
    #[must_use]
    pub fn with_dont_ask(mut self, label: AzString, checked: bool) -> Self {
        self.set_dont_ask(label, checked);
        self
    }
}

dialog_theme_and_dom!(
    MessageBox,
    build_message_box,
    MessageBox::create(
        MessageBoxKind::Info,
        AzString::from_const_str(""),
        AzString::from_const_str("")
    )
);

fn build_message_box(m: MessageBox, look: &DialogKitLook) -> Dom {
    if true {
        return Dom::create_div();
    } // RED stub
    let theme = dialog_kit::inner_theme(m.theme);
    let glyph_skin = match m.kind {
        MessageBoxKind::Info => &look.icon_info,
        MessageBoxKind::Warning => &look.icon_warning,
        MessageBoxKind::Error => &look.icon_error,
        MessageBoxKind::Question => &look.icon_question,
    };
    let mut words: Vec<Dom> = alloc::vec![dialog_kit::line(m.title.clone(), &[], &{
        let mut v = look.heading.clone();
        v.extend_from_slice(&look.block);
        v
    })];
    words.extend(paragraphs(&m.text, &look.text, look));
    if !m.detail.as_str().is_empty() {
        words.push(dialog_kit::line(m.detail.clone(), &[], &look.hint));
    }
    let mut content: Vec<Dom> = alloc::vec![Dom::create_div()
        .with_css_props(dialog_kit::part(ROW_TOP_BASE, &[]))
        .with_children(DomVec::from_vec(alloc::vec![
            Dom::create_icon(AzString::from_const_str(m.kind.icon()))
                .with_css_props(dialog_kit::part(FIXED_BASE, glyph_skin)),
            Dom::create_div()
                .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &[]))
                .with_children(DomVec::from_vec(words)),
        ]))];
    if !m.dont_ask_label.as_str().is_empty() {
        content.push(check(
            &m.dont_ask_label,
            m.dont_ask,
            StandardDialogEventKind::DontAskAgain,
            0,
            &m.on_event,
            theme,
            look,
        ));
    }
    let buttons: Vec<Dom> = m
        .buttons
        .as_ref()
        .iter()
        .enumerate()
        .map(|(i, label)| {
            button(
                label,
                i == m.default_button,
                StandardDialogEventKind::Button,
                i,
                true,
                &m.on_event,
                theme,
                look,
            )
        })
        .collect();
    // A message that asks for an answer: an alert named by its title.
    let a11y = AccessibilityInfo {
        description: Some(m.text.clone()).into(),
        ..AccessibilityInfo::named(m.title.clone(), AccessibilityRole::Alert)
    };
    body(MESSAGE_BOX_CLASS, a11y, content, buttons, look)
}

// ---------------------------------------------------------------------------
// AboutDialog
// ---------------------------------------------------------------------------

/// The About box: the icon, the name, the version, a line about the app,
/// the copyright, and the credits (the libraries it is built on and their
/// licenses).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AboutDialog {
    /// The icon glyph (a `Dom::create_icon` name), or empty for none.
    pub icon: AzString,
    /// The app's name.
    pub name: AzString,
    /// The version ("2.4.1").
    pub version: AzString,
    /// A line about the app, or empty.
    pub description: AzString,
    /// "Copyright 2026 ...", or empty.
    pub copyright: AzString,
    /// The heading over the credits ("Credits").
    pub credits_label: AzString,
    /// The close button's label ("OK").
    pub close_label: AzString,
    /// The credits: a component's name -> its license ("azul" -> "MIT").
    pub credits: StringPairVec,
    /// Hears `Button(0)` (the close button).
    pub on_event: OptionStandardDialogOnEvent,
    /// The widget theme this dialog is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
}

impl AboutDialog {
    /// The About box of `name` at `version`.
    #[must_use]
    pub fn create(name: AzString, version: AzString) -> Self {
        Self {
            icon: AzString::from_const_str(""),
            name,
            version,
            description: AzString::from_const_str(""),
            copyright: AzString::from_const_str(""),
            credits_label: AzString::from_const_str("Credits"),
            close_label: AzString::from_const_str("OK"),
            credits: StringPairVec::from_const_slice(&[]),
            on_event: None.into(),
            theme: OptionUiTheme::None,
        }
    }

    /// The icon glyph.
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// The line about the app.
    pub fn set_description(&mut self, description: AzString) {
        self.description = description;
    }

    /// [`Self::set_description`] for the builder chain.
    #[must_use]
    pub fn with_description(mut self, description: AzString) -> Self {
        self.set_description(description);
        self
    }

    /// The copyright line.
    pub fn set_copyright(&mut self, copyright: AzString) {
        self.copyright = copyright;
    }

    /// [`Self::set_copyright`] for the builder chain.
    #[must_use]
    pub fn with_copyright(mut self, copyright: AzString) -> Self {
        self.set_copyright(copyright);
        self
    }

    /// Appends a credit: `component` under `license`.
    pub fn add_credit(&mut self, component: AzString, license: AzString) {
        let mut v = self.credits.clone().into_library_owned_vec();
        v.push(azul_core::window::AzStringPair {
            key: component,
            value: license,
        });
        self.credits = StringPairVec::from_vec(v);
    }

    /// [`Self::add_credit`] for the builder chain.
    #[must_use]
    pub fn with_credit(mut self, component: AzString, license: AzString) -> Self {
        self.add_credit(component, license);
        self
    }

    /// The labels: over the credits, on the close button.
    pub fn set_labels(&mut self, credits_label: AzString, close_label: AzString) {
        self.credits_label = credits_label;
        self.close_label = close_label;
    }

    /// [`Self::set_labels`] for the builder chain.
    #[must_use]
    pub fn with_labels(mut self, credits_label: AzString, close_label: AzString) -> Self {
        self.set_labels(credits_label, close_label);
        self
    }
}

dialog_theme_and_dom!(
    AboutDialog,
    build_about,
    AboutDialog::create(AzString::from_const_str(""), AzString::from_const_str(""))
);

fn build_about(a: AboutDialog, look: &DialogKitLook) -> Dom {
    if true {
        return Dom::create_div();
    } // RED stub
    let theme = dialog_kit::inner_theme(a.theme);
    let with_block = |skin: &[azul_css::dynamic_selector::CssPropertyWithConditions]| {
        let mut v = skin.to_vec();
        v.extend_from_slice(&look.block);
        v
    };
    let mut content: Vec<Dom> = Vec::new();
    if !a.icon.as_str().is_empty() {
        content.push(
            Dom::create_icon(a.icon.clone())
                .with_css_props(dialog_kit::part(FIXED_BASE, &with_block(&look.logo))),
        );
    }
    content.push(dialog_kit::line(a.name.clone(), &[], &look.heading));
    content.push(dialog_kit::line(
        a.version.clone(),
        &[],
        &with_block(&look.hint),
    ));
    content.extend(paragraphs(&a.description, &look.text, look));
    if !a.copyright.as_str().is_empty() {
        content.push(dialog_kit::line(
            a.copyright.clone(),
            &[],
            &with_block(&look.hint),
        ));
    }
    if !a.credits.as_ref().is_empty() {
        content.push(dialog_kit::line(a.credits_label.clone(), &[], &look.label));
        let rows: Vec<Dom> = a
            .credits
            .as_ref()
            .iter()
            .map(|c| {
                Dom::create_div()
                    .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &look.list_row))
                    .with_accessibility_info(AccessibilityInfo::named(
                        AzString::from(alloc::format!("{}, {}", c.key.as_str(), c.value.as_str())),
                        AccessibilityRole::ListItem,
                    ))
                    .with_children(DomVec::from_vec(alloc::vec![
                        dialog_kit::line(c.key.clone(), GROW_LABEL_BASE, &look.text),
                        dialog_kit::line(c.value.clone(), FIXED_BASE, &look.size),
                    ]))
            })
            .collect();
        content.push(
            Dom::create_div()
                .with_ids_and_classes(dialog_kit::class(ABOUT_CREDITS_CLASS))
                .with_css_props(dialog_kit::part(SCROLL_BOX_BASE, &look.scroll_box))
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_info(AccessibilityInfo::named(
                    a.credits_label.clone(),
                    AccessibilityRole::List,
                ))
                .with_children(DomVec::from_vec(rows)),
        );
    }
    let buttons = alloc::vec![button(
        &a.close_label,
        true,
        StandardDialogEventKind::Button,
        0,
        true,
        &a.on_event,
        theme,
        look,
    )];
    let a11y = AccessibilityInfo::named(
        AzString::from(alloc::format!("About {}", a.name.as_str())),
        AccessibilityRole::Grouping,
    );
    body(ABOUT_CLASS, a11y, content, buttons, look)
}

#[cfg(test)]
mod standard_dialog_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks as tc},
    };

    type Log = Arc<Mutex<Vec<StandardDialogEvent>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: StandardDialogEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    fn new_log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn texts(node: &Dom) -> Vec<String> {
        tc::nodes(node)
            .into_iter()
            .filter_map(|(_, n)| match n.root.get_node_type() {
                NodeType::Text(s) if !s.as_str().is_empty() => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    fn id(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    /// Clicks the first node at or above the text `label` that takes a
    /// click; `None` when nothing does (an inert button).
    fn click(dom: Dom, label: &str) -> Option<(Update, Vec<crate::callbacks::CallbackChange>)> {
        let styled = StyledDom::create_from_dom(dom);
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let text = nodes
            .iter()
            .position(|n| matches!(n.get_node_type(), NodeType::Text(s) if s.as_str() == label))
            .unwrap_or_else(|| panic!("no text {label:?}"));
        let mut node = Some(NodeId::new(text));
        while let Some(n) = node {
            if nodes[n.index()]
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click))
            {
                return rv::fire(
                    &styled,
                    id(n.index()),
                    EventFilter::Hover(HoverEventFilter::Click),
                );
            }
            node = hierarchy[n.index()].parent_id();
        }
        None
    }

    fn strs(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    fn message(log: &Log) -> MessageBox {
        MessageBox::create(
            MessageBoxKind::Question,
            AzString::from("Save changes to Report.docx?"),
            AzString::from("Your changes will be lost if you don't save them."),
        )
        .with_buttons(strs(&["Save", "Don't Save", "Cancel"]), 0)
        .with_dont_ask(AzString::from("Don't ask again"), false)
        .with_on_event(
            RefAny::new(log.clone()),
            record as StandardDialogOnEventCallbackType,
        )
    }

    #[test]
    fn a_message_box_is_its_glyph_its_words_its_buttons_and_dont_ask_again() {
        let log = new_log();
        for theme in checks::BOTH {
            let dom = message(&log).with_theme(theme).dom();
            assert!(tc::has_class(&dom, MESSAGE_BOX_CLASS), "{}", theme.name());
            let info = dom.root.get_accessibility_info().expect("a role");
            assert_eq!(info.role, AccessibilityRole::Alert);
            assert_eq!(
                info.accessibility_name.as_ref().map(|s| s.as_str()),
                Some("Save changes to Report.docx?")
            );
            let glyph =
                tc::nodes(&dom)
                    .into_iter()
                    .find_map(|(_, n)| match n.root.get_node_type() {
                        NodeType::Icon(name) => Some(name.as_str().to_string()),
                        _ => None,
                    });
            assert_eq!(glyph.as_deref(), Some("help"), "a question's glyph");
            assert_eq!(
                texts(&dom),
                vec![
                    "Save changes to Report.docx?",
                    "Your changes will be lost if you don't save them.",
                    "Don't ask again",
                    "Save",
                    "Don't Save",
                    "Cancel"
                ],
                "{}",
                theme.name()
            );
        }
        for (kind, glyph) in [
            (MessageBoxKind::Info, "info"),
            (MessageBoxKind::Warning, "warning"),
            (MessageBoxKind::Error, "error"),
        ] {
            assert_eq!(kind.icon(), glyph);
        }
    }

    #[test]
    fn a_message_box_reports_the_button_and_dont_ask_again() {
        let log = new_log();
        click(message(&log).with_theme(UiTheme::Flat).dom(), "Don't Save").expect("a button");
        click(
            message(&log).with_theme(UiTheme::Flat).dom(),
            "Don't ask again",
        )
        .expect("the label");
        let got = log.lock().expect("log").clone();
        assert_eq!(
            (got[0].kind, got[0].index),
            (StandardDialogEventKind::Button, 1)
        );
        assert_eq!(
            (got[1].kind, got[1].checked),
            (StandardDialogEventKind::DontAskAgain, true)
        );
    }

    #[test]
    fn the_about_box_names_the_app_its_version_and_its_credits() {
        let log = new_log();
        for theme in checks::BOTH {
            let dom =
                AboutDialog::create(AzString::from("AzOffice"), AzString::from("Version 1.0.0"))
                    .with_icon(AzString::from("apps"))
                    .with_description(AzString::from("Documents, sheets and slides."))
                    .with_copyright(AzString::from("Copyright 2026 Azul contributors"))
                    .with_credit(AzString::from("azul"), AzString::from("MIT"))
                    .with_credit(
                        AzString::from("Material Icons"),
                        AzString::from("Apache-2.0"),
                    )
                    .with_on_event(
                        RefAny::new(log.clone()),
                        record as StandardDialogOnEventCallbackType,
                    )
                    .with_theme(theme)
                    .dom();
            assert_eq!(
                texts(&dom),
                vec![
                    "AzOffice",
                    "Version 1.0.0",
                    "Documents, sheets and slides.",
                    "Copyright 2026 Azul contributors",
                    "Credits",
                    "azul",
                    "MIT",
                    "Material Icons",
                    "Apache-2.0",
                    "OK"
                ],
                "{}",
                theme.name()
            );
            let credits = tc::find(&dom, ABOUT_CREDITS_CLASS).expect("the credits list");
            assert_eq!(
                credits.root.get_tab_index(),
                Some(TabIndex::Auto),
                "the list scrolls by keyboard"
            );
            assert!(tc::has_focus_ring(credits, false) && tc::has_focus_ring(credits, true));
            click(dom, "OK").expect("OK closes");
        }
        assert_eq!(
            log.lock().expect("log")[0].kind,
            StandardDialogEventKind::Button
        );
    }
}
