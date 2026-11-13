//! Standard dialogs - the BODIES of the dialogs every desktop app shows:
//!
//! | dialog                  | what it shows                                                       |
//! |-------------------------|---------------------------------------------------------------------|
//! | [`MessageBox`]          | an info / warning / error / question glyph, a title, the text, the steps it confirms, the buttons (one may be destructive), "Don't ask again" |
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
use core::fmt::Write as _;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::Update,
    dom::{Dom, DomVec, TabIndex},
    refany::RefAny,
    window::{StringPairVec, VirtualKeyCode},
};
use azul_css::{corety::OptionUsize, AzString, StringVec};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        alert::AlertKind,
        button::ButtonType,
        check_box::CheckBoxState,
        dialog_kit::{
            self, DialogKitLook, BUTTON_BOX_BASE, BUTTON_BOX_CLASS, BUTTON_ROW_BASE, FIXED_BASE,
            ROW_MIDDLE_BASE, ROW_TOP_BASE, SCROLL_BOX_BASE, SPACER_BASE,
        },
        info_bar::InfoBar,
        progressbar::ProgressBar,
        shells::{COLUMN_BASE, GROW_COLUMN_BASE, GROW_LABEL_BASE},
        spinner::Spinner,
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnTextInputCallbackType,
            TextInputOnVirtualKeyDownCallbackType, TextInputState, TextInputValid,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The message box's class.
pub const MESSAGE_BOX_CLASS: &str = "__azul-native-message-box";
/// The class of a message box's list of steps.
pub const MESSAGE_BOX_STEPS_CLASS: &str = "__azul-native-message-box-steps";
/// The class of one step of that list.
pub const MESSAGE_BOX_STEP_CLASS: &str = "__azul-native-message-box-step";
/// What a destructive button tells assistive technology when the app gave
/// no warning of its own.
pub const DESTRUCTIVE_REASON: &str = "This cannot be undone.";
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
    pub const fn create(kind: StandardDialogEventKind, index: usize, checked: bool) -> Self {
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

/// Why a `ProgressDialog`'s Cancel waits (`can_cancel` unset).
const CANCEL_HELD_REASON: &str = "This task cannot be cancelled now.";
/// Why a `LoginDialog`'s Sign in waits (a field is empty).
const SIGN_IN_HELD_REASON: &str = "Enter your user name and password.";
/// Why a `FindReplaceDialog`'s find and replace buttons wait (no text).
const FIND_HELD_REASON: &str = "Type the text to find.";

/// A button of the row: `primary` or not, reporting `kind` at `index`, or -
/// `held` names why - disabled (the Button's own disabled state: it keeps
/// its Tab stop and shows the reason on hover and on keyboard focus).
#[allow(clippy::too_many_arguments)]
fn button(
    label: &AzString,
    primary: bool,
    kind: StandardDialogEventKind,
    index: usize,
    held: Option<&'static str>,
    on_event: &OptionStandardDialogOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    let face = if primary {
        ButtonType::Primary
    } else {
        ButtonType::Default
    };
    button_in(label, face, kind, index, held, on_event, theme, look)
}

/// [`button`] in the Button face `face` (a message box's destructive
/// button wears [`ButtonType::Danger`]).
#[allow(clippy::too_many_arguments)]
fn button_in(
    label: &AzString,
    face: ButtonType,
    kind: StandardDialogEventKind,
    index: usize,
    held: Option<&'static str>,
    on_event: &OptionStandardDialogOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    dialog_kit::row_button(
        label.clone(),
        face,
        dialog_kit::RowAction::enabled_or(
            held.is_none(),
            || (report(on_event, kind, index, false), on_button),
            AzString::from_const_str(held.unwrap_or("")),
        ),
        theme,
        BUTTON_BOX_CLASS,
        BUTTON_BOX_BASE,
        &look.button,
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
        (report(on_event, kind, index, checked), on_check, on_check_label),
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

/// A message box: a glyph, a title, the text, the steps of what it
/// confirms, a detail line, the buttons (one of them may be destructive)
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
    /// What the confirmed action is about to do, one step each, numbered
    /// under the text; empty for no list.
    pub steps: StringVec,
    /// The buttons, left to right ("Save", "Don't Save", "Cancel").
    pub buttons: StringVec,
    /// The "Don't ask again" label, or empty for no checkbox.
    pub dont_ask_label: AzString,
    /// What the destructive button tells assistive technology, or empty
    /// for [`DESTRUCTIVE_REASON`].
    pub destructive_warning: AzString,
    /// Hears `Button(index)` and `DontAskAgain`.
    pub on_event: OptionStandardDialogOnEvent,
    /// The button whose action deletes, overwrites or costs money, if any:
    /// it wears the theme's danger face.
    pub destructive_button: OptionUsize,
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
            steps: StringVec::from_const_slice(&[]),
            buttons: StringVec::from_vec(alloc::vec![AzString::from_const_str("OK")]),
            dont_ask_label: AzString::from_const_str(""),
            destructive_warning: AzString::from_const_str(""),
            on_event: None.into(),
            destructive_button: OptionUsize::None,
            default_button: 0,
            kind,
            theme: OptionUiTheme::None,
            dont_ask: false,
        }
    }

    /// What the confirmed action is about to do, one step each ("Order 3 x
    /// SX65-2 at Hetzner, EUR 104.00 a month each", "Restart the token
    /// server"): numbered under the text, a list for assistive technology
    /// and part of what the box says when it opens. Empty: no list.
    pub fn set_steps(&mut self, steps: StringVec) {
        self.steps = steps;
    }

    /// [`Self::set_steps`] for the builder chain.
    #[must_use]
    pub fn with_steps(mut self, steps: StringVec) -> Self {
        self.set_steps(steps);
        self
    }

    /// Marks button `index` destructive - it deletes, overwrites or costs
    /// money: it wears the theme's danger face (red; flora's clay) instead
    /// of the plain or the primary one, and assistive technology hears
    /// `warning` as its description ("Deletes 3 files for good."; empty:
    /// "This cannot be undone."), since the colour tells it nothing.
    pub fn set_destructive_button(&mut self, index: usize, warning: AzString) {
        self.destructive_button = OptionUsize::Some(index);
        self.destructive_warning = warning;
    }

    /// [`Self::set_destructive_button`] for the builder chain.
    #[must_use]
    pub fn with_destructive_button(mut self, index: usize, warning: AzString) -> Self {
        self.set_destructive_button(index, warning);
        self
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
    if !m.steps.as_ref().is_empty() {
        words.push(steps_list(&m.steps, look));
    }
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
    let destructive = m.destructive_button.into_option();
    let warning = if m.destructive_warning.as_str().is_empty() {
        AzString::from_const_str(DESTRUCTIVE_REASON)
    } else {
        m.destructive_warning.clone()
    };
    let buttons: Vec<Dom> = m
        .buttons
        .as_ref()
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let face = if destructive == Some(i) {
                ButtonType::Danger
            } else if i == m.default_button {
                ButtonType::Primary
            } else {
                ButtonType::Default
            };
            let boxed = button_in(
                label,
                face,
                StandardDialogEventKind::Button,
                i,
                None,
                &m.on_event,
                theme,
                look,
            );
            if destructive == Some(i) {
                described(boxed, warning.clone())
            } else {
                boxed
            }
        })
        .collect();
    // A message that asks for an answer: an alert named by its title, which
    // says its text and the steps it confirms.
    let a11y = AccessibilityInfo {
        description: Some(alert_description(&m.text, &m.steps)).into(),
        ..AccessibilityInfo::named(m.title.clone(), AccessibilityRole::Alert)
    };
    body(MESSAGE_BOX_CLASS, a11y, content, buttons, look)
}

/// The steps a message box confirms: a list, one row per step - its number
/// in a column the texts line up after, its text beside it (a long step
/// wraps under itself, not under the number), each read with its number.
fn steps_list(steps: &StringVec, look: &DialogKitLook) -> Dom {
    let rows: Vec<Dom> = steps
        .as_ref()
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let number = alloc::format!("{}.", i + 1);
            Dom::create_div()
                .with_ids_and_classes(dialog_kit::class(MESSAGE_BOX_STEP_CLASS))
                .with_css_props(dialog_kit::part(ROW_TOP_BASE, &look.step))
                .with_accessibility_info(AccessibilityInfo::named(
                    AzString::from(alloc::format!("{number} {}", step.as_str())),
                    AccessibilityRole::ListItem,
                ))
                .with_children(DomVec::from_vec(alloc::vec![
                    dialog_kit::line(AzString::from(number), FIXED_BASE, &look.step_number),
                    dialog_kit::line(step.clone(), GROW_LABEL_BASE, &look.text),
                ]))
        })
        .collect();
    Dom::create_div()
        .with_ids_and_classes(dialog_kit::class(MESSAGE_BOX_STEPS_CLASS))
        .with_css_props(dialog_kit::part(COLUMN_BASE, &look.block))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::List,
            ..AccessibilityInfo::default()
        })
        .with_children(DomVec::from_vec(rows))
}

/// What a message box says when it opens: its text, then the steps it
/// confirms, one numbered line each.
fn alert_description(text: &AzString, steps: &StringVec) -> AzString {
    if steps.as_ref().is_empty() {
        return text.clone();
    }
    let mut said = String::from(text.as_str().trim_end());
    for (i, step) in steps.as_ref().iter().enumerate() {
        if !said.is_empty() {
            said.push('\n');
        }
        let _ = write!(said, "{}. {}", i + 1, step.as_str());
    }
    AzString::from(said)
}

/// `boxed` - a button row's box - with its Button described to assistive
/// technology by `description` (a destructive button's warning).
fn described(mut boxed: Dom, description: AzString) -> Dom {
    let kids: &mut [Dom] = boxed.children.as_mut();
    if let Some(button) = kids.first_mut() {
        let mut a11y = button
            .root
            .get_accessibility_info()
            .cloned()
            .unwrap_or_default();
        a11y.description = Some(description).into();
        button.root.set_accessibility_info(a11y);
    }
    boxed
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
        None,
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

// ---------------------------------------------------------------------------
// Fields (the login and find / replace dialogs)
// ---------------------------------------------------------------------------

extern "C" fn on_field_text(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let update = match data.downcast_ref::<Report>() {
        Some(r) => {
            let mut event =
                StandardDialogEvent::create(StandardDialogEventKind::FieldChanged, r.index, false);
            event.text = AzString::from(state.get_text());
            emit(&r.on_event, info, event)
        }
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Enter in a field asks what the field's `enter` says (sign in, find next).
extern "C" fn on_field_key(
    mut data: RefAny,
    info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match data.downcast_ref::<Report>() {
        Some(r) if key == Some(VirtualKeyCode::Return) => match r.enter {
            Some(kind) => emit(
                &r.on_event,
                info,
                StandardDialogEvent::create(kind, r.index, false),
            ),
            None => Update::DoNothing,
        },
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// A labelled field: the label over the text input (named by the label),
/// reporting `FieldChanged` at `index` and, on Enter, `enter`.
#[allow(clippy::too_many_arguments)]
fn field(
    label: &AzString,
    text: &AzString,
    index: usize,
    password: bool,
    enter: Option<StandardDialogEventKind>,
    on_event: &OptionStandardDialogOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    let data = RefAny::new(Report {
        on_event: on_event.clone(),
        kind: StandardDialogEventKind::FieldChanged,
        index,
        checked: false,
        enter,
    });
    let on_text: TextInputOnTextInputCallbackType = on_field_text;
    let on_key: TextInputOnVirtualKeyDownCallbackType = on_field_key;
    let mut input = if password {
        TextInput::create_password()
    } else {
        TextInput::create()
    }
    .with_text(text.clone())
    .with_accessibility_name(label.clone())
    .with_on_text_input(data.clone(), on_text)
    .with_on_virtual_key_down(data, on_key);
    if let Some(t) = theme {
        input = input.with_theme(t);
    }
    Dom::create_div()
        .with_css_props(dialog_kit::part(COLUMN_BASE, &look.block))
        .with_children(DomVec::from_vec(alloc::vec![
            dialog_kit::line(label.clone(), &[], &look.label),
            input.dom(),
        ]))
}

// ---------------------------------------------------------------------------
// ProgressDialog
// ---------------------------------------------------------------------------

/// A progress dialog: what is being done, a bar (or a spinner while the end
/// is unknown), the current item, Cancel.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ProgressDialog {
    /// The title ("Copying 12 files").
    pub title: AzString,
    /// The status line under it, or empty.
    pub text: AzString,
    /// The current item in the secondary ink ("report.docx"), or empty.
    pub detail: AzString,
    /// Cancel's label, or empty for no Cancel.
    pub cancel_label: AzString,
    /// Hears `Cancel`.
    pub on_event: OptionStandardDialogOnEvent,
    /// How far along, 0 to 100 (unused while `indeterminate`).
    pub percent: f32,
    /// The widget theme this dialog is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
    /// Whether the end is unknown (a spinner in the bar's place).
    pub indeterminate: bool,
    /// Whether Cancel does anything (unset, it is disabled and says why).
    pub can_cancel: bool,
}

impl ProgressDialog {
    /// A progress dialog titled `title` at `percent`, cancellable.
    #[must_use]
    pub fn create(title: AzString, percent: f32) -> Self {
        Self {
            title,
            text: AzString::from_const_str(""),
            detail: AzString::from_const_str(""),
            cancel_label: AzString::from_const_str("Cancel"),
            on_event: None.into(),
            percent,
            theme: OptionUiTheme::None,
            indeterminate: false,
            can_cancel: true,
        }
    }

    /// The status line.
    pub fn set_text(&mut self, text: AzString) {
        self.text = text;
    }

    /// [`Self::set_text`] for the builder chain.
    #[must_use]
    pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// The current item.
    pub fn set_detail(&mut self, detail: AzString) {
        self.detail = detail;
    }

    /// [`Self::set_detail`] for the builder chain.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.set_detail(detail);
        self
    }

    /// How far along.
    pub const fn set_percent(&mut self, percent: f32) {
        self.percent = percent;
    }

    /// [`Self::set_percent`] for the builder chain.
    #[must_use]
    pub const fn with_percent(mut self, percent: f32) -> Self {
        self.set_percent(percent);
        self
    }

    /// Whether the end is unknown.
    pub const fn set_indeterminate(&mut self, indeterminate: bool) {
        self.indeterminate = indeterminate;
    }

    /// [`Self::set_indeterminate`] for the builder chain.
    #[must_use]
    pub const fn with_indeterminate(mut self, indeterminate: bool) -> Self {
        self.set_indeterminate(indeterminate);
        self
    }

    /// Cancel: its label (empty: none) and whether it does anything.
    pub fn set_cancel(&mut self, label: AzString, can_cancel: bool) {
        self.cancel_label = label;
        self.can_cancel = can_cancel;
    }

    /// [`Self::set_cancel`] for the builder chain.
    #[must_use]
    pub fn with_cancel(mut self, label: AzString, can_cancel: bool) -> Self {
        self.set_cancel(label, can_cancel);
        self
    }
}

dialog_theme_and_dom!(
    ProgressDialog,
    build_progress_dialog,
    ProgressDialog::create(AzString::from_const_str(""), 0.0)
);

fn build_progress_dialog(p: ProgressDialog, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(p.theme);
    let mut content: Vec<Dom> = alloc::vec![dialog_kit::line(p.title.clone(), &[], &{
        let mut v = look.heading.clone();
        v.extend_from_slice(&look.block);
        v
    })];
    content.extend(paragraphs(&p.text, &look.text, look));
    let gauge = if p.indeterminate {
        let mut spinner = Spinner::create();
        if let Some(t) = theme {
            spinner = spinner.with_theme(t);
        }
        spinner.dom()
    } else {
        let mut bar = ProgressBar::create(p.percent.clamp(0.0, 100.0))
            .with_accessibility_name(p.title.clone());
        if let Some(t) = theme {
            bar = bar.with_theme(t);
        }
        Dom::create_div()
            .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &[]))
            .with_children(DomVec::from_vec(alloc::vec![
                Dom::create_div()
                    .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &[]))
                    .with_child(bar.dom()),
                dialog_kit::line(dialog_kit::percent_text(p.percent), FIXED_BASE, &look.unit),
            ]))
    };
    content.push(
        Dom::create_div()
            .with_css_props(dialog_kit::part(COLUMN_BASE, &look.block))
            .with_child(gauge),
    );
    if !p.detail.as_str().is_empty() {
        content.push(dialog_kit::line(p.detail.clone(), &[], &look.hint));
    }
    let mut buttons: Vec<Dom> = Vec::new();
    if !p.cancel_label.as_str().is_empty() {
        buttons.push(button(
            &p.cancel_label,
            false,
            StandardDialogEventKind::Cancel,
            0,
            (!p.can_cancel).then_some(CANCEL_HELD_REASON),
            &p.on_event,
            theme,
            look,
        ));
    }
    // Busy until the end: a group named by the title.
    let a11y = AccessibilityInfo {
        states: azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
            azul_core::a11y::AccessibilityState::Busy
        ]),
        ..AccessibilityInfo::named(p.title, AccessibilityRole::Grouping)
    };
    body(PROGRESS_DIALOG_CLASS, a11y, content, buttons, look)
}

// ---------------------------------------------------------------------------
// LoginDialog
// ---------------------------------------------------------------------------

/// A login dialog: a message, the user name and password, "Remember me",
/// an error line, Sign in / Cancel. It STORES nothing: the app keeps the
/// two texts from the `FieldChanged` reports, hands them on at `Submit`
/// (to the server, to the OS keyring when "Remember me" is ticked) and
/// clears the password.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct LoginDialog {
    /// The title ("Sign in to `AzOffice`").
    pub title: AzString,
    /// A line under the title, or empty.
    pub message: AzString,
    /// "User name".
    pub user_label: AzString,
    /// "Password".
    pub password_label: AzString,
    /// The user name typed so far.
    pub user: AzString,
    /// The password typed so far (shown as dots).
    pub password: AzString,
    /// "Remember me", or empty for no checkbox.
    pub remember_label: AzString,
    /// Why the last attempt failed, or empty.
    pub error: AzString,
    /// "Sign in".
    pub submit_label: AzString,
    /// "Cancel".
    pub cancel_label: AzString,
    /// Hears `FieldChanged` (0 user, 1 password), `OptionToggled` (0
    /// remember), `Submit` and `Cancel`.
    pub on_event: OptionStandardDialogOnEvent,
    /// The widget theme this dialog is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
    /// Whether "Remember me" is ticked.
    pub remember: bool,
}

impl LoginDialog {
    /// A login dialog titled `title`, empty.
    #[must_use]
    pub fn create(title: AzString) -> Self {
        Self {
            title,
            message: AzString::from_const_str(""),
            user_label: AzString::from_const_str("User name"),
            password_label: AzString::from_const_str("Password"),
            user: AzString::from_const_str(""),
            password: AzString::from_const_str(""),
            remember_label: AzString::from_const_str(""),
            error: AzString::from_const_str(""),
            submit_label: AzString::from_const_str("Sign in"),
            cancel_label: AzString::from_const_str("Cancel"),
            on_event: None.into(),
            theme: OptionUiTheme::None,
            remember: false,
        }
    }

    /// The line under the title.
    pub fn set_message(&mut self, message: AzString) {
        self.message = message;
    }

    /// [`Self::set_message`] for the builder chain.
    #[must_use]
    pub fn with_message(mut self, message: AzString) -> Self {
        self.set_message(message);
        self
    }

    /// The texts typed so far.
    pub fn set_credentials(&mut self, user: AzString, password: AzString) {
        self.user = user;
        self.password = password;
    }

    /// [`Self::set_credentials`] for the builder chain.
    #[must_use]
    pub fn with_credentials(mut self, user: AzString, password: AzString) -> Self {
        self.set_credentials(user, password);
        self
    }

    /// "Remember me": its label (empty: none) and whether it is ticked.
    pub fn set_remember(&mut self, label: AzString, checked: bool) {
        self.remember_label = label;
        self.remember = checked;
    }

    /// [`Self::set_remember`] for the builder chain.
    #[must_use]
    pub fn with_remember(mut self, label: AzString, checked: bool) -> Self {
        self.set_remember(label, checked);
        self
    }

    /// Why the last attempt failed (empty: no error).
    pub fn set_error(&mut self, error: AzString) {
        self.error = error;
    }

    /// [`Self::set_error`] for the builder chain.
    #[must_use]
    pub fn with_error(mut self, error: AzString) -> Self {
        self.set_error(error);
        self
    }

    /// The labels: the two fields, Sign in, Cancel.
    pub fn set_labels(
        &mut self,
        user_label: AzString,
        password_label: AzString,
        submit_label: AzString,
        cancel_label: AzString,
    ) {
        self.user_label = user_label;
        self.password_label = password_label;
        self.submit_label = submit_label;
        self.cancel_label = cancel_label;
    }

    /// [`Self::set_labels`] for the builder chain.
    #[must_use]
    pub fn with_labels(
        mut self,
        user_label: AzString,
        password_label: AzString,
        submit_label: AzString,
        cancel_label: AzString,
    ) -> Self {
        self.set_labels(user_label, password_label, submit_label, cancel_label);
        self
    }

    /// Whether Sign in does anything: a user name and a password.
    #[must_use]
    pub fn can_submit(&self) -> bool {
        !self.user.as_str().trim().is_empty() && !self.password.as_str().is_empty()
    }
}

dialog_theme_and_dom!(
    LoginDialog,
    build_login,
    LoginDialog::create(AzString::from_const_str(""))
);

fn build_login(l: LoginDialog, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(l.theme);
    let submit = Some(StandardDialogEventKind::Submit);
    let mut content: Vec<Dom> = alloc::vec![dialog_kit::line(l.title.clone(), &[], &{
        let mut v = look.heading.clone();
        v.extend_from_slice(&look.block);
        v
    })];
    content.extend(paragraphs(&l.message, &look.text, look));
    if !l.error.as_str().is_empty() {
        let mut bar = InfoBar::create(l.error.clone())
            .with_kind(AlertKind::Danger)
            .with_icon(AzString::from_const_str("error"));
        if let Some(t) = theme {
            bar = bar.with_theme(t);
        }
        content.push(
            Dom::create_div()
                .with_css_props(dialog_kit::part(COLUMN_BASE, &look.block))
                .with_child(bar.dom()),
        );
    }
    content.push(field(
        &l.user_label,
        &l.user,
        0,
        false,
        submit,
        &l.on_event,
        theme,
        look,
    ));
    content.push(field(
        &l.password_label,
        &l.password,
        1,
        true,
        submit,
        &l.on_event,
        theme,
        look,
    ));
    if !l.remember_label.as_str().is_empty() {
        content.push(check(
            &l.remember_label,
            l.remember,
            StandardDialogEventKind::OptionToggled,
            0,
            &l.on_event,
            theme,
            look,
        ));
    }
    let buttons = alloc::vec![
        button(
            &l.cancel_label,
            false,
            StandardDialogEventKind::Cancel,
            0,
            None,
            &l.on_event,
            theme,
            look
        ),
        button(
            &l.submit_label,
            true,
            StandardDialogEventKind::Submit,
            0,
            (!l.can_submit()).then_some(SIGN_IN_HELD_REASON),
            &l.on_event,
            theme,
            look,
        ),
    ];
    let a11y = AccessibilityInfo::named(l.title, AccessibilityRole::Grouping);
    body(LOGIN_CLASS, a11y, content, buttons, look)
}

// ---------------------------------------------------------------------------
// FindReplaceDialog
// ---------------------------------------------------------------------------

/// A find / replace dialog: the text to find (and its replacement), match
/// case, whole word, the result ("3 of 12"), the buttons.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct FindReplaceDialog {
    /// The text to find.
    pub find: AzString,
    /// The replacement.
    pub replace: AzString,
    /// The result line ("3 of 12", "No matches"), or empty.
    pub status: AzString,
    /// "Find what:".
    pub find_label: AzString,
    /// "Replace with:".
    pub replace_label: AzString,
    /// "Match case".
    pub match_case_label: AzString,
    /// "Whole word".
    pub whole_word_label: AzString,
    /// "Find next".
    pub find_next_label: AzString,
    /// "Find previous".
    pub find_previous_label: AzString,
    /// "Replace".
    pub replace_button_label: AzString,
    /// "Replace all".
    pub replace_all_label: AzString,
    /// "Close".
    pub close_label: AzString,
    /// Hears `FieldChanged` (0 find, 1 replace), `OptionToggled` (0 match
    /// case, 1 whole word), `FindNext`, `FindPrevious`, `Replace`,
    /// `ReplaceAll` and `Cancel`.
    pub on_event: OptionStandardDialogOnEvent,
    /// The widget theme this dialog is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
    /// Whether the replace field and buttons show.
    pub show_replace: bool,
    /// Whether "Match case" is ticked.
    pub match_case: bool,
    /// Whether "Whole word" is ticked.
    pub whole_word: bool,
}

impl FindReplaceDialog {
    /// A find dialog for `find` (replace hidden).
    #[must_use]
    pub fn create(find: AzString) -> Self {
        Self {
            find,
            replace: AzString::from_const_str(""),
            status: AzString::from_const_str(""),
            find_label: AzString::from_const_str("Find what:"),
            replace_label: AzString::from_const_str("Replace with:"),
            match_case_label: AzString::from_const_str("Match case"),
            whole_word_label: AzString::from_const_str("Whole word"),
            find_next_label: AzString::from_const_str("Find next"),
            find_previous_label: AzString::from_const_str("Find previous"),
            replace_button_label: AzString::from_const_str("Replace"),
            replace_all_label: AzString::from_const_str("Replace all"),
            close_label: AzString::from_const_str("Close"),
            on_event: None.into(),
            theme: OptionUiTheme::None,
            show_replace: false,
            match_case: false,
            whole_word: false,
        }
    }

    /// The replacement, and the replace field and buttons shown.
    pub fn set_replace(&mut self, replace: AzString) {
        self.replace = replace;
        self.show_replace = true;
    }

    /// [`Self::set_replace`] for the builder chain.
    #[must_use]
    pub fn with_replace(mut self, replace: AzString) -> Self {
        self.set_replace(replace);
        self
    }

    /// The result line.
    pub fn set_status(&mut self, status: AzString) {
        self.status = status;
    }

    /// [`Self::set_status`] for the builder chain.
    #[must_use]
    pub fn with_status(mut self, status: AzString) -> Self {
        self.set_status(status);
        self
    }

    /// The options: match case, whole word.
    pub const fn set_options(&mut self, match_case: bool, whole_word: bool) {
        self.match_case = match_case;
        self.whole_word = whole_word;
    }

    /// [`Self::set_options`] for the builder chain.
    #[must_use]
    pub const fn with_options(mut self, match_case: bool, whole_word: bool) -> Self {
        self.set_options(match_case, whole_word);
        self
    }
}

dialog_theme_and_dom!(
    FindReplaceDialog,
    build_find_replace,
    FindReplaceDialog::create(AzString::from_const_str(""))
);

fn build_find_replace(f: FindReplaceDialog, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(f.theme);
    // Nothing to find: the find and replace buttons wait, and say why.
    let find_held = f.find.as_str().is_empty().then_some(FIND_HELD_REASON);
    let mut content: Vec<Dom> = alloc::vec![field(
        &f.find_label,
        &f.find,
        0,
        false,
        Some(StandardDialogEventKind::FindNext),
        &f.on_event,
        theme,
        look,
    )];
    if f.show_replace {
        content.push(field(
            &f.replace_label,
            &f.replace,
            1,
            false,
            None,
            &f.on_event,
            theme,
            look,
        ));
    }
    content.push(check(
        &f.match_case_label,
        f.match_case,
        StandardDialogEventKind::OptionToggled,
        0,
        &f.on_event,
        theme,
        look,
    ));
    content.push(check(
        &f.whole_word_label,
        f.whole_word,
        StandardDialogEventKind::OptionToggled,
        1,
        &f.on_event,
        theme,
        look,
    ));
    if !f.status.as_str().is_empty() {
        content.push(dialog_kit::line(f.status.clone(), &[], &look.hint));
    }
    let mut buttons = alloc::vec![
        button(
            &f.find_previous_label,
            false,
            StandardDialogEventKind::FindPrevious,
            0,
            find_held,
            &f.on_event,
            theme,
            look,
        ),
        button(
            &f.find_next_label,
            true,
            StandardDialogEventKind::FindNext,
            0,
            find_held,
            &f.on_event,
            theme,
            look
        ),
    ];
    if f.show_replace {
        buttons.push(button(
            &f.replace_button_label,
            false,
            StandardDialogEventKind::Replace,
            0,
            find_held,
            &f.on_event,
            theme,
            look,
        ));
        buttons.push(button(
            &f.replace_all_label,
            false,
            StandardDialogEventKind::ReplaceAll,
            0,
            find_held,
            &f.on_event,
            theme,
            look,
        ));
    }
    buttons.push(button(
        &f.close_label,
        false,
        StandardDialogEventKind::Cancel,
        0,
        None,
        &f.on_event,
        theme,
        look,
    ));
    let name = if f.show_replace {
        "Find and replace"
    } else {
        "Find"
    };
    let a11y = AccessibilityInfo::named(name, AccessibilityRole::Grouping);
    body(FIND_REPLACE_CLASS, a11y, content, buttons, look)
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

    /// The reason a click showed (a disabled button's tooltip), if any.
    fn says_why(clicked: Option<(Update, Vec<crate::callbacks::CallbackChange>)>) -> Option<String> {
        clicked?.1.into_iter().find_map(|c| match c {
            crate::callbacks::CallbackChange::ShowTooltip { text, .. } if !text.as_str().is_empty() => {
                Some(text.as_str().to_string())
            }
            _ => None,
        })
    }

    /// User decision D1 (2026-10-05): every disabled button of a standard
    /// dialog is the Button's own disabled state with a reason - it keeps
    /// its Tab stop and is described by the reason (shown on hover and on
    /// keyboard focus) - and no button box dims it a second time.
    #[test]
    fn every_disabled_standard_dialog_button_keeps_its_tab_stop_and_says_why() {
        use crate::widgets::button::BUTTON_DISABLED_CLASS;
        for theme in checks::BOTH {
            let held = [
                (
                    "progress: Cancel",
                    ProgressDialog::create(AzString::from("Copying"), 30.0)
                        .with_cancel(AzString::from("Cancel"), false)
                        .with_theme(theme)
                        .dom(),
                ),
                (
                    "login: Sign in",
                    LoginDialog::create(AzString::from("Sign in"))
                        .with_theme(theme)
                        .dom(),
                ),
                (
                    "find: the find buttons",
                    FindReplaceDialog::create(AzString::from(""))
                        .with_replace(AzString::from(""))
                        .with_theme(theme)
                        .dom(),
                ),
            ];
            for (what, dom) in held {
                let disabled = tc::find_all(&dom, BUTTON_DISABLED_CLASS);
                assert!(
                    !disabled.is_empty(),
                    "{} {what}: the Button's own disabled state",
                    theme.name()
                );
                for button in disabled {
                    assert_eq!(
                        button.root.get_tab_index(),
                        Some(TabIndex::Auto),
                        "{} {what}: keeps its Tab stop",
                        theme.name()
                    );
                    let why = button
                        .root
                        .get_accessibility_info()
                        .and_then(|a| a.description.as_ref().map(|d| d.as_str().to_string()))
                        .unwrap_or_default();
                    assert!(!why.is_empty(), "{} {what}: says why", theme.name());
                }
                for button_box in tc::find_all(&dom, BUTTON_BOX_CLASS) {
                    assert!(
                        !button_box.root.style.iter_inline_properties().any(|(p, _)| matches!(
                            p,
                            azul_css::props::property::CssProperty::Opacity(_)
                        )),
                        "{} {what}: no box dims a button a second time",
                        theme.name()
                    );
                }
            }
        }
    }

    /// Clicks the first node at or above the text `label` that takes a
    /// click; `None` when nothing does.
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

    /// A confirmation in a message box: what it is about to do, numbered
    /// under the text, and the button that orders in the danger face.
    fn confirm(log: &Log) -> MessageBox {
        MessageBox::create(
            MessageBoxKind::Warning,
            AzString::from("Order 3 x SX65 at Hetzner Robot?"),
            AzString::from("Hetzner bills from the order on."),
        )
        .with_steps(strs(&[
            "Order 3 x SX65-2 at Hetzner, EUR 104.00 a month each",
            "Wait until the servers show up",
            "Provision each into a slot",
        ]))
        .with_detail(AzString::from("azctl block order --yes"))
        .with_buttons(strs(&["Cancel", "Order"]), 1)
        .with_destructive_button(1, AzString::from("Costs money: EUR 312.00 a month."))
        .with_on_event(
            RefAny::new(log.clone()),
            record as StandardDialogOnEventCallbackType,
        )
    }

    /// What assistive technology hears after a node's name and role.
    fn description_of(node: &Dom) -> Option<String> {
        node.root
            .get_accessibility_info()
            .and_then(|a| a.description.as_ref().map(|d| d.as_str().to_string()))
    }

    #[test]
    fn a_message_box_numbers_the_steps_of_what_it_confirms_under_its_text() {
        let log = new_log();
        for theme in checks::BOTH {
            let dom = confirm(&log).with_theme(theme).dom();
            assert_eq!(
                texts(&dom),
                vec![
                    "Order 3 x SX65 at Hetzner Robot?",
                    "Hetzner bills from the order on.",
                    "1.",
                    "Order 3 x SX65-2 at Hetzner, EUR 104.00 a month each",
                    "2.",
                    "Wait until the servers show up",
                    "3.",
                    "Provision each into a slot",
                    "azctl block order --yes",
                    "Cancel",
                    "Order"
                ],
                "{}: the title, the text, the steps, the detail, the buttons",
                theme.name()
            );
            let list = tc::find(&dom, MESSAGE_BOX_STEPS_CLASS).expect("the steps");
            assert_eq!(
                list.root.get_accessibility_info().map(|a| a.role),
                Some(AccessibilityRole::List),
                "{}",
                theme.name()
            );
            let items: Vec<(AccessibilityRole, String)> = list
                .children
                .as_ref()
                .iter()
                .filter_map(|c| c.root.get_accessibility_info())
                .map(|a| {
                    let name = a
                        .accessibility_name
                        .as_ref()
                        .map(|n| n.as_str().to_string())
                        .unwrap_or_default();
                    (a.role, name)
                })
                .collect();
            assert_eq!(
                items,
                vec![
                    (
                        AccessibilityRole::ListItem,
                        String::from("1. Order 3 x SX65-2 at Hetzner, EUR 104.00 a month each")
                    ),
                    (
                        AccessibilityRole::ListItem,
                        String::from("2. Wait until the servers show up")
                    ),
                    (
                        AccessibilityRole::ListItem,
                        String::from("3. Provision each into a slot")
                    ),
                ],
                "{}: every step a list item read with its number",
                theme.name()
            );
            let alert = description_of(&dom).unwrap_or_default();
            assert!(
                alert.starts_with("Hetzner bills from the order on.")
                    && alert.contains("1. Order 3 x SX65-2")
                    && alert.contains("3. Provision each into a slot"),
                "{}: the alert says what will happen: {alert:?}",
                theme.name()
            );
        }
        let plain = message(&log).with_theme(UiTheme::Flat).dom();
        assert!(
            tc::find(&plain, MESSAGE_BOX_STEPS_CLASS).is_none(),
            "no steps, no list"
        );
        assert_eq!(
            description_of(&plain).as_deref(),
            Some("Your changes will be lost if you don't save them."),
            "without steps the alert says its text"
        );
    }

    /// The destructive button (it orders, deletes, overwrites) is the
    /// Button's danger face in either theme - and a screen reader hears
    /// why, which the colour cannot tell it. It still reports its index.
    #[test]
    fn a_destructive_button_wears_the_danger_face_and_tells_a_screen_reader_why() {
        let log = new_log();
        for theme in checks::BOTH {
            let dom = confirm(&log).with_theme(theme).dom();
            let danger = tc::find_all(&dom, ButtonType::Danger.class_name());
            assert_eq!(danger.len(), 1, "{}: one destructive button", theme.name());
            assert_eq!(texts(danger[0]), vec!["Order"], "{}", theme.name());
            assert_eq!(
                description_of(danger[0]).as_deref(),
                Some("Costs money: EUR 312.00 a month."),
                "{}: the warning is its description",
                theme.name()
            );
            assert!(
                tc::find(&dom, ButtonType::Primary.class_name()).is_none(),
                "{}: the destructive default button wears the danger face, not the accent",
                theme.name()
            );
            let cancel = tc::find(&dom, ButtonType::Default.class_name()).expect("Cancel");
            assert_eq!(texts(cancel), vec!["Cancel"], "{}", theme.name());
            assert_eq!(
                description_of(cancel),
                None,
                "{}: Cancel destroys nothing",
                theme.name()
            );
        }

        let delete = MessageBox::create(
            MessageBoxKind::Warning,
            AzString::from("Delete Report.docx?"),
            AzString::from("It goes for good."),
        )
        .with_buttons(strs(&["Delete", "Cancel"]), 1)
        .with_destructive_button(0, AzString::from(""))
        .with_theme(UiTheme::Flat)
        .dom();
        let button = tc::find(&delete, ButtonType::Danger.class_name()).expect("Delete");
        assert_eq!(texts(button), vec!["Delete"]);
        assert_eq!(
            description_of(button).as_deref(),
            Some(DESTRUCTIVE_REASON),
            "no warning given: the general one"
        );
        let default = tc::find(&delete, ButtonType::Primary.class_name()).expect("Cancel");
        assert_eq!(texts(default), vec!["Cancel"], "Cancel stays the default");

        click(confirm(&log).with_theme(UiTheme::Flat).dom(), "Order").expect("the button");
        let got = log.lock().expect("log").clone();
        assert_eq!(
            (got[0].kind, got[0].index),
            (StandardDialogEventKind::Button, 1)
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

    #[test]
    fn the_progress_dialog_shows_a_bar_or_a_spinner_and_cancel() {
        let log = new_log();
        let progress = || {
            ProgressDialog::create(AzString::from("Copying 12 files"), 42.0)
                .with_text(AzString::from("From Downloads to Documents"))
                .with_detail(AzString::from("report.docx"))
                .with_on_event(
                    RefAny::new(log.clone()),
                    record as StandardDialogOnEventCallbackType,
                )
        };
        for theme in checks::BOTH {
            let dom = progress().with_theme(theme).dom();
            assert_eq!(
                texts(&dom),
                vec![
                    "Copying 12 files",
                    "From Downloads to Documents",
                    "42 %",
                    "report.docx",
                    "Cancel"
                ],
                "{}",
                theme.name()
            );
            let spinning = progress().with_indeterminate(true).with_theme(theme).dom();
            assert!(
                !texts(&spinning).iter().any(|t| t == "42 %"),
                "no percentage without an end"
            );
            assert!(!tc::nodes(&spinning).into_iter().any(|(_, n)| n
                .root
                .get_accessibility_info()
                .is_some_and(|i| i.role == AccessibilityRole::ProgressBar)));
        }
        click(progress().with_theme(UiTheme::Flat).dom(), "Cancel").expect("Cancel");
        assert_eq!(
            log.lock().expect("log")[0].kind,
            StandardDialogEventKind::Cancel
        );
        let before = log.lock().expect("log").len();
        assert!(
            says_why(click(
                progress()
                    .with_cancel(AzString::from("Cancel"), false)
                    .with_theme(UiTheme::Flat)
                    .dom(),
                "Cancel"
            ))
            .is_some(),
            "a held Cancel says why"
        );
        assert_eq!(
            log.lock().expect("log").len(),
            before,
            "and reports nothing"
        );
    }

    #[test]
    fn the_login_dialog_signs_in_only_with_both_fields_and_stores_nothing() {
        let log = new_log();
        let login = || {
            LoginDialog::create(AzString::from("Sign in to AzOffice"))
                .with_remember(AzString::from("Remember me"), false)
                .with_on_event(
                    RefAny::new(log.clone()),
                    record as StandardDialogOnEventCallbackType,
                )
        };
        assert!(!login().can_submit());
        assert!(!login()
            .with_credentials(AzString::from("  "), AzString::from("pw"))
            .can_submit());
        let ready = login().with_credentials(AzString::from("felix"), AzString::from("pw"));
        assert!(ready.can_submit());
        for theme in checks::BOTH {
            let dom = login().with_theme(theme).dom();
            let all = texts(&dom);
            for want in [
                "Sign in to AzOffice",
                "User name",
                "Password",
                "Remember me",
                "Cancel",
                "Sign in",
            ] {
                assert!(
                    all.iter().any(|t| t == want),
                    "{}: {want} in {all:?}",
                    theme.name()
                );
            }
            let failed = login()
                .with_error(AzString::from("The password is wrong."))
                .with_theme(theme)
                .dom();
            assert!(tc::nodes(&failed).into_iter().any(|(_, n)| n
                .root
                .get_accessibility_info()
                .is_some_and(|i| i.role == AccessibilityRole::Alert)));
        }
        assert!(
            says_why(click(login().with_theme(UiTheme::Flat).dom(), "Sign in")).is_some(),
            "Sign in says why while a field is empty (and reports nothing: the log below)"
        );
        click(ready.with_theme(UiTheme::Flat).dom(), "Sign in").expect("Sign in");
        click(login().with_theme(UiTheme::Flat).dom(), "Remember me").expect("the label");
        let got = log.lock().expect("log").clone();
        assert_eq!(got[0].kind, StandardDialogEventKind::Submit);
        assert_eq!(
            (got[1].kind, got[1].index, got[1].checked),
            (StandardDialogEventKind::OptionToggled, 0, true)
        );
    }

    #[test]
    fn the_find_dialog_finds_only_with_a_text_and_shows_replace_on_request() {
        let log = new_log();
        let find = |text: &str| {
            FindReplaceDialog::create(AzString::from(text))
                .with_status(AzString::from("3 of 12"))
                .with_on_event(
                    RefAny::new(log.clone()),
                    record as StandardDialogOnEventCallbackType,
                )
        };
        for theme in checks::BOTH {
            let all = texts(&find("azul").with_theme(theme).dom());
            for want in [
                "Find what:",
                "Match case",
                "Whole word",
                "3 of 12",
                "Find previous",
                "Find next",
                "Close",
            ] {
                assert!(all.iter().any(|t| t == want), "{}: {want}", theme.name());
            }
            assert!(!all.iter().any(|t| t == "Replace all"), "replace is hidden");
            let replacing = texts(
                &find("azul")
                    .with_replace(AzString::from("Azul"))
                    .with_theme(theme)
                    .dom(),
            );
            for want in ["Replace with:", "Replace", "Replace all"] {
                assert!(
                    replacing.iter().any(|t| t == want),
                    "{}: {want}",
                    theme.name()
                );
            }
        }
        assert!(
            says_why(click(find("").with_theme(UiTheme::Flat).dom(), "Find next")).is_some(),
            "Find next says why without a text (and reports nothing: the log below)"
        );
        click(find("azul").with_theme(UiTheme::Flat).dom(), "Find next").expect("Find next");
        click(find("azul").with_theme(UiTheme::Flat).dom(), "Whole word").expect("the label");
        click(find("").with_theme(UiTheme::Flat).dom(), "Close").expect("Close");
        let got = log.lock().expect("log").clone();
        let kinds: Vec<StandardDialogEventKind> = got.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                StandardDialogEventKind::FindNext,
                StandardDialogEventKind::OptionToggled,
                StandardDialogEventKind::Cancel
            ]
        );
        assert_eq!((got[1].index, got[1].checked), (1, true));
    }

    fn pinned<T>(mut dialog: T, theme: OptionUiTheme, set: fn(&mut T, UiTheme)) -> T {
        if let OptionUiTheme::Some(t) = theme {
            set(&mut dialog, t);
        }
        dialog
    }

    fn message_dom(t: OptionUiTheme) -> Dom {
        pinned(message(&new_log()), t, MessageBox::set_theme).dom()
    }

    fn confirm_dom(t: OptionUiTheme) -> Dom {
        pinned(confirm(&new_log()), t, MessageBox::set_theme).dom()
    }

    fn about_dom(t: OptionUiTheme) -> Dom {
        pinned(
            AboutDialog::create(AzString::from("AzOffice"), AzString::from("1.0"))
                .with_icon(AzString::from("apps"))
                .with_credit(AzString::from("azul"), AzString::from("MIT")),
            t,
            AboutDialog::set_theme,
        )
        .dom()
    }

    fn progress_dom(t: OptionUiTheme) -> Dom {
        pinned(
            ProgressDialog::create(AzString::from("Copying"), 30.0),
            t,
            ProgressDialog::set_theme,
        )
        .dom()
    }

    fn login_dom(t: OptionUiTheme) -> Dom {
        pinned(
            LoginDialog::create(AzString::from("Sign in"))
                .with_error(AzString::from("Wrong password."))
                .with_remember(AzString::from("Remember me"), true),
            t,
            LoginDialog::set_theme,
        )
        .dom()
    }

    fn find_dom(t: OptionUiTheme) -> Dom {
        pinned(
            FindReplaceDialog::create(AzString::from("azul")).with_replace(AzString::from("Azul")),
            t,
            FindReplaceDialog::set_theme,
        )
        .dom()
    }

    type DialogFn = fn(OptionUiTheme) -> Dom;

    #[test]
    fn every_standard_dialog_follows_the_app_theme() {
        for (name, dialog) in [
            ("message_box", message_dom as DialogFn),
            ("message_box (steps, destructive)", confirm_dom as DialogFn),
            ("about_dialog", about_dom as DialogFn),
            ("progress_dialog", progress_dom as DialogFn),
            ("login_dialog", login_dom as DialogFn),
            ("find_replace_dialog", find_dom as DialogFn),
        ] {
            checks::assert_follows_the_app_theme(
                name,
                || dialog(OptionUiTheme::None),
                |t: UiTheme| dialog(OptionUiTheme::Some(t)),
            );
        }
    }
}
