//! Wizard pages - the reusable pages of an install wizard, each the `Dom`
//! a [`WizardLayout`](crate::widgets::wizard_layout::WizardLayout) shows as
//! its page:
//!
//! | page                     | what it shows                                                  |
//! |--------------------------|----------------------------------------------------------------|
//! | [`WizardWelcomePage`]    | a logo, a title, the welcome text                              |
//! | [`WizardLicensePage`]    | the license in a box that scrolls, "I accept" gating Next      |
//! | [`WizardDestinationPage`]| the folder ([`PathInput`]: field + Browse), required / free space, a warning when short |
//! | [`WizardComponentsPage`] | a checklist tree of components with sizes and the total        |
//! | [`WizardOptionsPage`]    | checkbox and radio rows                                        |
//! | [`WizardSummaryPage`]    | the read-only review of every choice                           |
//! | [`WizardProgressPage`]   | the progress bar, the current item, a details log              |
//! | [`WizardFinishPage`]     | a title, the text, "Launch now" / "Open readme" checkboxes     |
//!
//! The pages own nothing: the app keeps every value and rebuilds. A page
//! reports what the user did through ONE callback type
//! ([`WizardPageOnEventCallbackType`], a [`WizardPageEvent`] saying what and
//! where), and the pages whose input decides whether the wizard may go on
//! answer the VALIDATION HOOK: `blocked_reason()` is empty when Next may
//! go, else the sentence the wizard shows beside its held Next
//! (`WizardLayout::set_validation`). The data rules an app would otherwise
//! write itself are the pages' own: [`WizardComponentsPage::toggle`] (a
//! group carries its children, a child its group, a required component
//! stays), [`WizardComponentsPage::total_bytes`],
//! [`WizardOptionsPage::choose`] (one radio per group).
//!
//! [`PathInput`]: crate::widgets::path_input::PathInput
//!
//! Key types: the eight pages, [`WizardPageEvent`], [`WizardComponent`],
//! [`WizardOption`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::Update,
    dom::{Dom, DomVec, TabIndex},
    refany::RefAny,
    window::{AzStringPair, StringPairVec},
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{layout::LayoutPaddingLeft, property::CssProperty},
    AzString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        alert::AlertKind,
        button::{Button, ButtonOnClickCallbackType},
        check_box::{CheckBoxOnToggleCallbackType, CheckBoxState},
        dialog_kit::{self, DialogKitLook, FIXED_BASE, ROW_MIDDLE_BASE, SCROLL_BOX_BASE},
        info_bar::InfoBar,
        path_input::{PathInput, PathInputOnChangeCallbackType},
        progressbar::ProgressBar,
        radio_group::{RadioGroup, RadioGroupOnChangeCallbackType, RadioGroupState},
        shells::{COLUMN_BASE, GROW_COLUMN_BASE},
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The welcome page's class.
pub const WELCOME_CLASS: &str = "__azul-native-wizard-welcome";
/// The license page's class.
pub const LICENSE_CLASS: &str = "__azul-native-wizard-license";
/// The class of the box the license text scrolls in.
pub const LICENSE_TEXT_CLASS: &str = "__azul-native-wizard-license-text";
/// The destination page's class.
pub const DESTINATION_CLASS: &str = "__azul-native-wizard-destination";
/// The class of the "Space required / available" lines.
pub const DESTINATION_SPACE_CLASS: &str = "__azul-native-wizard-destination-space";
/// The components page's class.
pub const COMPONENTS_CLASS: &str = "__azul-native-wizard-components";
/// The class of one component row.
pub const COMPONENT_ROW_CLASS: &str = "__azul-native-wizard-component";
/// The class of the total under the components.
pub const COMPONENTS_TOTAL_CLASS: &str = "__azul-native-wizard-components-total";
/// The options page's class.
pub const OPTIONS_CLASS: &str = "__azul-native-wizard-options";
/// The class of one checkbox row (an option, a finish action, the license's
/// "I accept").
pub const CHECK_ROW_CLASS: &str = "__azul-native-wizard-check-row";
/// The summary page's class.
pub const SUMMARY_CLASS: &str = "__azul-native-wizard-summary";
/// The class of one summary entry.
pub const SUMMARY_ROW_CLASS: &str = "__azul-native-wizard-summary-row";
/// The progress page's class.
pub const PROGRESS_CLASS: &str = "__azul-native-wizard-progress";
/// The class of the current item line ("Copying azword.dll").
pub const PROGRESS_ITEM_CLASS: &str = "__azul-native-wizard-progress-item";
/// The class of the details log.
pub const PROGRESS_LOG_CLASS: &str = "__azul-native-wizard-progress-log";
/// The finish page's class.
pub const FINISH_CLASS: &str = "__azul-native-wizard-finish";

// ---------------------------------------------------------------------------
// The event
// ---------------------------------------------------------------------------

/// What the user did on a page.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WizardPageEventKind {
    /// The license's "I accept" changed: `checked` is the new value.
    LicenseAccepted,
    /// The destination folder changed (typed or picked): `text` is the path.
    PathChanged,
    /// Component `index` was ticked (`checked`) or cleared.
    ComponentToggled,
    /// Option `index` (an options page's, a finish page's) was ticked or
    /// cleared; a radio reports the one chosen, `checked` set.
    OptionToggled,
    /// The progress page's details were shown (`checked`) or hidden.
    DetailsToggled,
}

/// One report from a wizard page.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WizardPageEvent {
    /// The path (`PathChanged`), else empty.
    pub text: AzString,
    /// The component or option (`ComponentToggled`, `OptionToggled`), else 0.
    pub index: usize,
    /// What happened.
    pub kind: WizardPageEventKind,
    /// The new value of a checkbox (`LicenseAccepted`, `ComponentToggled`,
    /// `OptionToggled`, `DetailsToggled`).
    pub checked: bool,
}

impl WizardPageEvent {
    /// An event of `kind` at `index` with the new value `checked`.
    #[must_use]
    pub fn create(kind: WizardPageEventKind, index: usize, checked: bool) -> Self {
        Self {
            text: AzString::from_const_str(""),
            index,
            kind,
            checked,
        }
    }
}

/// Callback invoked for a report from a wizard page.
pub type WizardPageOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, WizardPageEvent) -> Update;
impl_widget_callback!(
    WizardPageOnEvent,
    OptionWizardPageOnEvent,
    WizardPageOnEventCallback,
    WizardPageOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        WizardPageOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: WIZARD_PAGE_ON_EVENT_INVOKER,
    invoker_ty:     AzWizardPageOnEventCallbackInvoker,
    thunk_fn:       az_wizard_page_on_event_callback_thunk,
    setter_fn:      AzApp_setWizardPageOnEventCallbackInvoker,
    from_handle_fn: AzWizardPageOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzWizardPageOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: WizardPageEvent ],
}

/// The callback slot every page with input has, set the same way.
fn on_event_of<C: Into<WizardPageOnEventCallback>>(
    data: RefAny,
    callback: C,
) -> OptionWizardPageOnEvent {
    Some(WizardPageOnEvent {
        refany: data,
        callback: callback.into(),
    })
    .into()
}

// ---------------------------------------------------------------------------
// The list items: components and options
// ---------------------------------------------------------------------------

/// One installable component: a row of the components page's checklist.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct WizardComponent {
    /// The component's name ("Spreadsheets").
    pub label: AzString,
    /// A line under the name, or empty.
    pub description: AzString,
    /// Its size on disk, in bytes (0 for a group that holds only children).
    pub size_bytes: u64,
    /// Its depth in the tree: 0 for a top-level component, 1 for one of its
    /// parts, ... The list is in tree order (a component's parts follow it).
    pub depth: u32,
    /// Whether it is ticked.
    pub checked: bool,
    /// Whether it cannot be cleared (the program files).
    pub required: bool,
}

impl_option!(
    WizardComponent,
    OptionWizardComponent,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    WizardComponent,
    WizardComponentVec,
    WizardComponentVecDestructor,
    WizardComponentVecDestructorType,
    WizardComponentVecSlice,
    OptionWizardComponent
);
impl_vec_clone!(
    WizardComponent,
    WizardComponentVec,
    WizardComponentVecDestructor
);
impl_vec_debug!(WizardComponent, WizardComponentVec);
impl_vec_partialeq!(WizardComponent, WizardComponentVec);
impl_vec_mut!(WizardComponent, WizardComponentVec);

impl WizardComponent {
    /// A ticked top-level component `label` of `size_bytes`.
    #[must_use]
    pub fn create(label: AzString, size_bytes: u64) -> Self {
        Self {
            label,
            description: AzString::from_const_str(""),
            size_bytes,
            depth: 0,
            checked: true,
            required: false,
        }
    }

    /// The line under the name.
    #[must_use]
    pub fn with_description(mut self, description: AzString) -> Self {
        self.description = description;
        self
    }

    /// The depth in the tree.
    #[must_use]
    pub const fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    /// Whether it is ticked.
    #[must_use]
    pub const fn with_checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Whether it cannot be cleared (a required component is ticked).
    #[must_use]
    pub const fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        if required {
            self.checked = true;
        }
        self
    }
}

/// One option: a checkbox row, or - with a group - one radio of a set.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct WizardOption {
    /// The option's label ("Create a desktop shortcut").
    pub label: AzString,
    /// A line under the label, or empty.
    pub description: AzString,
    /// 0 for an independent checkbox; options sharing a non-zero group are
    /// one radio set (exactly one is ticked).
    pub group: u32,
    /// Whether it is ticked (the chosen radio of its set).
    pub checked: bool,
}

impl_option!(
    WizardOption,
    OptionWizardOption,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    WizardOption,
    WizardOptionVec,
    WizardOptionVecDestructor,
    WizardOptionVecDestructorType,
    WizardOptionVecSlice,
    OptionWizardOption
);
impl_vec_clone!(WizardOption, WizardOptionVec, WizardOptionVecDestructor);
impl_vec_debug!(WizardOption, WizardOptionVec);
impl_vec_partialeq!(WizardOption, WizardOptionVec);
impl_vec_mut!(WizardOption, WizardOptionVec);

impl WizardOption {
    /// An independent checkbox option.
    #[must_use]
    pub fn create(label: AzString, checked: bool) -> Self {
        Self {
            label,
            description: AzString::from_const_str(""),
            group: 0,
            checked,
        }
    }

    /// The line under the label.
    #[must_use]
    pub fn with_description(mut self, description: AzString) -> Self {
        self.description = description;
        self
    }

    /// The radio set the option belongs to (0: none, a checkbox).
    #[must_use]
    pub const fn with_group(mut self, group: u32) -> Self {
        self.group = group;
        self
    }
}

/// Ticks (or clears) option `index` of `options`: a checkbox takes
/// `checked`; a radio, ticked, clears the rest of its set and cannot be
/// cleared on its own. The rule both the options and the finish page use.
fn choose_option(options: &mut WizardOptionVec, index: usize, checked: bool) {
    let mut v = core::mem::replace(options, WizardOptionVec::from_const_slice(&[]))
        .into_library_owned_vec();
    if let Some(group) = v.get(index).map(|o| o.group) {
        if group == 0 {
            v[index].checked = checked;
        } else if checked {
            for o in v.iter_mut().filter(|o| o.group == group) {
                o.checked = false;
            }
            v[index].checked = true;
        }
    }
    *options = WizardOptionVec::from_vec(v);
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

/// Hands `event` to the page's callback.
fn emit(on_event: &OptionWizardPageOnEvent, info: CallbackInfo, event: WizardPageEvent) -> Update {
    match on_event.as_ref() {
        Some(WizardPageOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, event)
        }
        None => Update::DoNothing,
    }
}

/// What a checkbox row's box and label share: the page's callback, the
/// report and the row's current value.
struct CheckRef {
    on_event: OptionWizardPageOnEvent,
    kind: WizardPageEventKind,
    index: usize,
    checked: bool,
}

/// The checkbox flipped itself: report its new value.
extern "C" fn on_check_toggle(
    mut data: RefAny,
    info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    let Some(r) = data.downcast_ref::<CheckRef>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        WizardPageEvent::create(r.kind, r.index, state.checked),
    )
}

/// The label was clicked: report the flipped value (the app rebuilds the
/// box with it).
extern "C" fn on_check_label(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(r) = data.downcast_ref::<CheckRef>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        WizardPageEvent::create(r.kind, r.index, !r.checked),
    )
}

/// What a radio set shares: the page's callback and the page index of each
/// radio.
struct RadioRef {
    on_event: OptionWizardPageOnEvent,
    indices: Vec<usize>,
}

extern "C" fn on_radio(mut data: RefAny, info: CallbackInfo, state: RadioGroupState) -> Update {
    let Some(r) = data.downcast_ref::<RadioRef>() else {
        return Update::DoNothing;
    };
    let Some(index) = r.indices.get(state.selected_index).copied() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        WizardPageEvent::create(WizardPageEventKind::OptionToggled, index, true),
    )
}

/// The destination's field or picker: report the path.
struct PathRef {
    on_event: OptionWizardPageOnEvent,
}

extern "C" fn on_path(mut data: RefAny, info: CallbackInfo, path: AzString) -> Update {
    let Some(r) = data.downcast_ref::<PathRef>() else {
        return Update::DoNothing;
    };
    let mut event = WizardPageEvent::create(WizardPageEventKind::PathChanged, 0, false);
    event.text = path;
    emit(&r.on_event, info, event)
}

/// The progress page's details button: report the flipped visibility.
extern "C" fn on_details(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(r) = data.downcast_ref::<CheckRef>() else {
        return Update::DoNothing;
    };
    emit(
        &r.on_event,
        info,
        WizardPageEvent::create(WizardPageEventKind::DetailsToggled, 0, !r.checked),
    )
}

// ---------------------------------------------------------------------------
// Shared parts of the build
// ---------------------------------------------------------------------------

/// A page's root: a growing column of the kit's page face.
fn page_root(class: &'static str, look: &DialogKitLook, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(dialog_kit::root_classes(class, look))
        .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &look.page))
        .with_children(DomVec::from_vec(children))
}

/// `skin` then the block gap: a part that is a block of a page.
fn with_block(
    skin: &[CssPropertyWithConditions],
    look: &DialogKitLook,
) -> Vec<CssPropertyWithConditions> {
    let mut v = skin.to_vec();
    v.extend_from_slice(&look.block);
    v
}

/// The widget's own text, one block per paragraph (`\n\n` apart).
fn paragraphs(
    text: &AzString,
    skin: &[CssPropertyWithConditions],
    look: &DialogKitLook,
) -> Vec<Dom> {
    let block = with_block(skin, look);
    text.as_str()
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| dialog_kit::line(AzString::from(p.trim()), &[], &block))
        .collect()
}

/// A glyph of the kit's logo face, or nothing for an empty name.
fn logo(icon: &AzString, look: &DialogKitLook) -> Option<Dom> {
    if icon.as_str().is_empty() {
        return None;
    }
    Some(
        Dom::create_icon(icon.clone())
            .with_css_props(dialog_kit::part(FIXED_BASE, &with_block(&look.logo, look))),
    )
}

/// A checkbox row: the box (named by the label) and the label beside it; a
/// click on either reports `kind` at `index` with the new value.
#[allow(clippy::too_many_arguments)]
fn check_row(
    label: &AzString,
    checked: bool,
    kind: WizardPageEventKind,
    index: usize,
    on_event: &OptionWizardPageOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    let shared = RefAny::new(CheckRef {
        on_event: on_event.clone(),
        kind,
        index,
        checked,
    });
    dialog_kit::check_row(
        label,
        checked,
        (
            shared,
            on_check_toggle as CheckBoxOnToggleCallbackType,
            on_check_label as ButtonOnClickCallbackType,
        ),
        CHECK_ROW_CLASS,
        theme,
        look,
    )
}

/// The option rows: an independent option a checkbox row (and its
/// description), each radio set ONE `RadioGroup` (and the chosen radio's
/// description), in list order.
fn option_rows(
    options: &WizardOptionVec,
    on_event: &OptionWizardPageOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Vec<Dom> {
    let list = options.as_ref();
    let mut out = Vec::new();
    let mut sets_built: Vec<u32> = Vec::new();
    for (i, o) in list.iter().enumerate() {
        if o.group == 0 {
            out.push(check_row(
                &o.label,
                o.checked,
                WizardPageEventKind::OptionToggled,
                i,
                on_event,
                theme,
                look,
            ));
            if !o.description.as_str().is_empty() {
                out.push(dialog_kit::line(
                    o.description.clone(),
                    &[],
                    &look.description,
                ));
            }
            continue;
        }
        // A radio set is built once, where its first option stands, from
        // every option of its group.
        if sets_built.contains(&o.group) {
            continue;
        }
        sets_built.push(o.group);
        let indices: Vec<usize> = (i..list.len())
            .filter(|&j| list[j].group == o.group)
            .collect();
        let labels: Vec<AzString> = indices.iter().map(|&j| list[j].label.clone()).collect();
        let chosen = indices.iter().position(|&j| list[j].checked).unwrap_or(0);
        let mut radios = RadioGroup::create(StringVec::from_vec(labels))
            .with_selected_index(chosen)
            .with_accessibility_name(o.label.clone())
            .with_on_change(
                RefAny::new(RadioRef {
                    on_event: on_event.clone(),
                    indices: indices.clone(),
                }),
                on_radio as RadioGroupOnChangeCallbackType,
            );
        if let Some(t) = theme {
            radios = radios.with_theme(t);
        }
        out.push(
            Dom::create_div()
                .with_css_props(dialog_kit::part(COLUMN_BASE, &look.check_row))
                .with_child(radios.dom()),
        );
        if let Some(&j) = indices.get(chosen) {
            if !list[j].description.as_str().is_empty() {
                out.push(dialog_kit::line(
                    list[j].description.clone(),
                    &[],
                    &look.description,
                ));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The pages
// ---------------------------------------------------------------------------

macro_rules! page_theme_and_dom {
    ($page:ident, $build:ident, $default:expr) => {
        impl $page {
            /// Pin the widget theme; unset, the page follows the app theme.
            pub const fn set_theme(&mut self, theme: UiTheme) {
                self.theme = OptionUiTheme::Some(theme);
            }

            /// [`Self::set_theme`] for the builder chain.
            #[must_use]
            pub const fn with_theme(mut self, theme: UiTheme) -> Self {
                self.set_theme(theme);
                self
            }

            /// Replaces `self` with an empty page and returns the original.
            #[must_use]
            pub fn swap_with_default(&mut self) -> Self {
                let mut s = $default;
                core::mem::swap(&mut s, self);
                s
            }

            /// The page's DOM.
            #[must_use]
            pub fn dom(self) -> Dom {
                let look = dialog_kit::look_for(self.theme);
                $build(self, &look)
            }
        }

        impl Default for $page {
            fn default() -> Self {
                $default
            }
        }

        impl From<$page> for Dom {
            fn from(p: $page) -> Self {
                p.dom()
            }
        }
    };
}

// ---- Welcome ----

/// The first page: a logo, a title and the welcome text.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct WizardWelcomePage {
    /// The logo glyph (a `Dom::create_icon` name), or empty for none.
    pub logo: AzString,
    /// The title ("Welcome to the AzOffice Setup Wizard").
    pub title: AzString,
    /// The text; paragraphs are `\n\n` apart.
    pub text: AzString,
    /// The widget theme this page is PINNED to, or `None` to follow the app
    /// theme.
    pub theme: OptionUiTheme,
}

impl WizardWelcomePage {
    /// A welcome page titled `title` saying `text`.
    #[must_use]
    pub fn create(title: AzString, text: AzString) -> Self {
        Self {
            logo: AzString::from_const_str(""),
            title,
            text,
            theme: OptionUiTheme::None,
        }
    }

    /// The logo glyph.
    pub fn set_logo(&mut self, logo: AzString) {
        self.logo = logo;
    }

    /// [`Self::set_logo`] for the builder chain.
    #[must_use]
    pub fn with_logo(mut self, logo: AzString) -> Self {
        self.set_logo(logo);
        self
    }
}

page_theme_and_dom!(
    WizardWelcomePage,
    build_welcome,
    WizardWelcomePage::create(AzString::from_const_str(""), AzString::from_const_str(""))
);

fn build_welcome(page: WizardWelcomePage, look: &DialogKitLook) -> Dom {
    let mut children: Vec<Dom> = Vec::new();
    children.extend(logo(&page.logo, look));
    children.push(dialog_kit::line(
        page.title,
        &[],
        &with_block(&look.heading, look),
    ));
    children.extend(paragraphs(&page.text, &look.text, look));
    page_root(WELCOME_CLASS, look, children)
}

// ---- License ----

/// The license page: the agreement in a box that scrolls, "I accept" under
/// it. Next is held until it is ticked.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WizardLicensePage {
    /// The agreement; paragraphs are `\n\n` apart.
    pub text: AzString,
    /// The line over the box ("Please read the following license
    /// agreement.").
    pub intro: AzString,
    /// The checkbox's label.
    pub accept_label: AzString,
    /// The reason Next is held while the box is clear.
    pub reason: AzString,
    /// Hears `LicenseAccepted`.
    pub on_event: OptionWizardPageOnEvent,
    /// The widget theme this page is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
    /// Whether "I accept" is ticked.
    pub accepted: bool,
}

impl WizardLicensePage {
    /// A license page showing `text`, not accepted yet.
    #[must_use]
    pub fn create(text: AzString) -> Self {
        Self {
            text,
            intro: AzString::from_const_str("Please read the following license agreement."),
            accept_label: AzString::from_const_str("I accept the terms of the license agreement"),
            reason: AzString::from_const_str("Accept the license agreement to continue."),
            on_event: None.into(),
            theme: OptionUiTheme::None,
            accepted: false,
        }
    }

    /// The line over the box.
    pub fn set_intro(&mut self, intro: AzString) {
        self.intro = intro;
    }

    /// [`Self::set_intro`] for the builder chain.
    #[must_use]
    pub fn with_intro(mut self, intro: AzString) -> Self {
        self.set_intro(intro);
        self
    }

    /// The checkbox's label and the reason Next is held without it.
    pub fn set_labels(&mut self, accept_label: AzString, reason: AzString) {
        self.accept_label = accept_label;
        self.reason = reason;
    }

    /// [`Self::set_labels`] for the builder chain.
    #[must_use]
    pub fn with_labels(mut self, accept_label: AzString, reason: AzString) -> Self {
        self.set_labels(accept_label, reason);
        self
    }

    /// Whether "I accept" is ticked.
    pub const fn set_accepted(&mut self, accepted: bool) {
        self.accepted = accepted;
    }

    /// [`Self::set_accepted`] for the builder chain.
    #[must_use]
    pub const fn with_accepted(mut self, accepted: bool) -> Self {
        self.set_accepted(accepted);
        self
    }

    /// The callback that hears `LicenseAccepted`.
    pub fn set_on_event<C: Into<WizardPageOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = on_event_of(data, callback);
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<WizardPageOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// The validation hook: empty once accepted, else [`Self::reason`].
    #[must_use]
    pub fn blocked_reason(&self) -> AzString {
        if self.accepted {
            AzString::from_const_str("")
        } else {
            self.reason.clone()
        }
    }
}

page_theme_and_dom!(
    WizardLicensePage,
    build_license,
    WizardLicensePage::create(AzString::from_const_str(""))
);

fn build_license(page: WizardLicensePage, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(page.theme);
    let mut children: Vec<Dom> = Vec::new();
    children.push(dialog_kit::line(
        page.intro,
        &[],
        &with_block(&look.text, look),
    ));
    // The agreement is the APP's text: plain paragraphs a user may select.
    let agreement: Vec<Dom> = page
        .text
        .as_str()
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| crate::widgets::widget_p_with_text(AzString::from(p.trim())))
        .collect();
    children.push(
        Dom::create_div()
            .with_ids_and_classes(dialog_kit::class(LICENSE_TEXT_CLASS))
            .with_css_props(dialog_kit::part(SCROLL_BOX_BASE, &with_block(&look.scroll_box, look)))
            // A box that scrolls is a keyboard stop, so the arrows scroll it.
            .with_tab_index(TabIndex::Auto)
            .with_accessibility_info(AccessibilityInfo::named(
                "License agreement",
                AccessibilityRole::Document,
            ))
            .with_children(DomVec::from_vec(agreement)),
    );
    children.push(check_row(
        &page.accept_label,
        page.accepted,
        WizardPageEventKind::LicenseAccepted,
        0,
        &page.on_event,
        theme,
        look,
    ));
    page_root(LICENSE_CLASS, look, children)
}

// ---- Destination ----

/// The destination page: the folder (field + Browse), the space the
/// components need and the space the drive has, a warning when it is short.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WizardDestinationPage {
    /// The folder.
    pub path: AzString,
    /// The line over the field ("Setup will install AzOffice into the
    /// following folder.").
    pub intro: AzString,
    /// The field's label ("Destination folder").
    pub field_label: AzString,
    /// The Browse button's label.
    pub browse_label: AzString,
    /// The picker's title.
    pub dialog_title: AzString,
    /// "Space required:".
    pub required_label: AzString,
    /// "Space available:".
    pub available_label: AzString,
    /// The reason Next is held while the field is empty.
    pub empty_reason: AzString,
    /// The reason Next is held while the drive is short (and the warning).
    pub short_reason: AzString,
    /// The bytes the chosen components need.
    pub required_bytes: u64,
    /// The bytes free on the folder's drive (see `available_known`).
    pub available_bytes: u64,
    /// Hears `PathChanged`.
    pub on_event: OptionWizardPageOnEvent,
    /// The widget theme this page is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
    /// Whether `available_bytes` is known (the app asked the drive:
    /// `file::disk_space_for_new`).
    pub available_known: bool,
}

impl WizardDestinationPage {
    /// A destination page for `path`, the components needing
    /// `required_bytes`; the free space is unknown.
    #[must_use]
    pub fn create(path: AzString, required_bytes: u64) -> Self {
        Self {
            path,
            intro: AzString::from_const_str(
                "Setup will install the program into the following folder.",
            ),
            field_label: AzString::from_const_str("Destination folder"),
            browse_label: AzString::from_const_str("Browse..."),
            dialog_title: AzString::from_const_str("Choose the destination folder"),
            required_label: AzString::from_const_str("Space required:"),
            available_label: AzString::from_const_str("Space available:"),
            empty_reason: AzString::from_const_str("Choose a folder to install into."),
            short_reason: AzString::from_const_str("There is not enough free space on the drive."),
            required_bytes,
            available_bytes: 0,
            on_event: None.into(),
            theme: OptionUiTheme::None,
            available_known: false,
        }
    }

    /// The bytes free on the folder's drive.
    pub const fn set_available(&mut self, bytes: u64) {
        self.available_bytes = bytes;
        self.available_known = true;
    }

    /// [`Self::set_available`] for the builder chain.
    #[must_use]
    pub const fn with_available(mut self, bytes: u64) -> Self {
        self.set_available(bytes);
        self
    }

    /// The line over the field.
    pub fn set_intro(&mut self, intro: AzString) {
        self.intro = intro;
    }

    /// [`Self::set_intro`] for the builder chain.
    #[must_use]
    pub fn with_intro(mut self, intro: AzString) -> Self {
        self.set_intro(intro);
        self
    }

    /// The labels: the field's, the Browse button's, the picker's title,
    /// "Space required:" and "Space available:".
    pub fn set_labels(
        &mut self,
        field_label: AzString,
        browse_label: AzString,
        dialog_title: AzString,
        required_label: AzString,
        available_label: AzString,
    ) {
        self.field_label = field_label;
        self.browse_label = browse_label;
        self.dialog_title = dialog_title;
        self.required_label = required_label;
        self.available_label = available_label;
    }

    /// [`Self::set_labels`] for the builder chain.
    #[must_use]
    pub fn with_labels(
        mut self,
        field_label: AzString,
        browse_label: AzString,
        dialog_title: AzString,
        required_label: AzString,
        available_label: AzString,
    ) -> Self {
        self.set_labels(
            field_label,
            browse_label,
            dialog_title,
            required_label,
            available_label,
        );
        self
    }

    /// The reasons Next is held: no folder, not enough space.
    pub fn set_reasons(&mut self, empty_reason: AzString, short_reason: AzString) {
        self.empty_reason = empty_reason;
        self.short_reason = short_reason;
    }

    /// [`Self::set_reasons`] for the builder chain.
    #[must_use]
    pub fn with_reasons(mut self, empty_reason: AzString, short_reason: AzString) -> Self {
        self.set_reasons(empty_reason, short_reason);
        self
    }

    /// The callback that hears `PathChanged`.
    pub fn set_on_event<C: Into<WizardPageOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = on_event_of(data, callback);
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<WizardPageOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Whether the drive is known to be short.
    #[must_use]
    pub const fn is_short(&self) -> bool {
        self.available_known && self.available_bytes < self.required_bytes
    }

    /// The validation hook: empty when Next may go, else why not (no
    /// folder, not enough space).
    #[must_use]
    pub fn blocked_reason(&self) -> AzString {
        if self.path.as_str().trim().is_empty() {
            self.empty_reason.clone()
        } else if self.is_short() {
            self.short_reason.clone()
        } else {
            AzString::from_const_str("")
        }
    }
}

page_theme_and_dom!(
    WizardDestinationPage,
    build_destination,
    WizardDestinationPage::create(AzString::from_const_str(""), 0)
);

fn build_destination(page: WizardDestinationPage, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(page.theme);
    let short = page.is_short();
    let mut children: Vec<Dom> = Vec::new();
    children.push(dialog_kit::line(
        page.intro.clone(),
        &[],
        &with_block(&look.text, look),
    ));
    children.push(dialog_kit::line(page.field_label.clone(), &[], &look.label));
    let mut field = PathInput::create(page.path.clone())
        .with_accessibility_name(page.field_label.clone())
        .with_browse_label(page.browse_label.clone())
        .with_dialog_title(page.dialog_title.clone())
        .with_on_change(
            RefAny::new(PathRef {
                on_event: page.on_event.clone(),
            }),
            on_path as PathInputOnChangeCallbackType,
        );
    if let Some(t) = theme {
        field = field.with_theme(t);
    }
    children.push(
        Dom::create_div()
            .with_css_props(dialog_kit::part(COLUMN_BASE, &look.block))
            .with_child(field.dom()),
    );
    let space = |label: &AzString, bytes: u64| {
        dialog_kit::line(
            AzString::from(alloc::format!("{} {}", label.as_str(), crate::file::DiskSpace::format_bytes(bytes))),
            &[],
            &look.hint,
        )
        .with_ids_and_classes(dialog_kit::class(DESTINATION_SPACE_CLASS))
    };
    children.push(space(&page.required_label, page.required_bytes));
    if page.available_known {
        children.push(space(&page.available_label, page.available_bytes));
    }
    if short {
        let mut warning = InfoBar::create(AzString::from(alloc::format!(
            "{} ({} needed, {} available)",
            page.short_reason.as_str(),
            crate::file::DiskSpace::format_bytes(page.required_bytes),
            crate::file::DiskSpace::format_bytes(page.available_bytes)
        )))
        .with_kind(AlertKind::Warning)
        .with_icon(AzString::from_const_str("warning"));
        if let Some(t) = theme {
            warning = warning.with_theme(t);
        }
        children.push(
            Dom::create_div()
                .with_css_props(dialog_kit::part(COLUMN_BASE, &look.block))
                .with_child(warning.dom()),
        );
    }
    page_root(DESTINATION_CLASS, look, children)
}

// ---- Components ----

/// The components page: a checklist tree of the components with their
/// sizes, and the total of the ticked ones.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WizardComponentsPage {
    /// The components, in tree order.
    pub components: WizardComponentVec,
    /// The line over the list ("Select the components to install.").
    pub intro: AzString,
    /// The label before the total ("Space required:").
    pub total_label: AzString,
    /// The reason Next is held while nothing is ticked.
    pub reason: AzString,
    /// Hears `ComponentToggled`.
    pub on_event: OptionWizardPageOnEvent,
    /// The widget theme this page is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
}

impl WizardComponentsPage {
    /// A components page over `components`.
    #[must_use]
    pub fn create(components: WizardComponentVec) -> Self {
        Self {
            components,
            intro: AzString::from_const_str("Select the components to install."),
            total_label: AzString::from_const_str("Space required:"),
            reason: AzString::from_const_str("Choose at least one component."),
            on_event: None.into(),
            theme: OptionUiTheme::None,
        }
    }

    /// The line over the list.
    pub fn set_intro(&mut self, intro: AzString) {
        self.intro = intro;
    }

    /// [`Self::set_intro`] for the builder chain.
    #[must_use]
    pub fn with_intro(mut self, intro: AzString) -> Self {
        self.set_intro(intro);
        self
    }

    /// The label before the total.
    pub fn set_total_label(&mut self, label: AzString) {
        self.total_label = label;
    }

    /// [`Self::set_total_label`] for the builder chain.
    #[must_use]
    pub fn with_total_label(mut self, label: AzString) -> Self {
        self.set_total_label(label);
        self
    }

    /// The callback that hears `ComponentToggled`.
    pub fn set_on_event<C: Into<WizardPageOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = on_event_of(data, callback);
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<WizardPageOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// The bytes of the ticked components.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.components
            .as_ref()
            .iter()
            .filter(|c| c.checked)
            .map(|c| c.size_bytes)
            .sum()
    }

    /// Ticks (`checked`) or clears component `index`: its parts follow it
    /// (a required part stays ticked), and every group above it is ticked
    /// while any of its parts is. A required component cannot be cleared.
    pub fn toggle(&mut self, index: usize, checked: bool) {
        let mut v = core::mem::replace(
            &mut self.components,
            WizardComponentVec::from_const_slice(&[]),
        )
        .into_library_owned_vec();
        if index < v.len() && (checked || !v[index].required) {
            v[index].checked = checked;
            let depth = v[index].depth;
            // The parts: every later row deeper than this one.
            let mut j = index + 1;
            while j < v.len() && v[j].depth > depth {
                v[j].checked = checked || v[j].required;
                j += 1;
            }
            // The groups above: ticked while any of their parts is.
            let mut level = depth;
            let mut k = index;
            while level > 0 && k > 0 {
                k -= 1;
                if v[k].depth < level {
                    level = v[k].depth;
                    let mut any = false;
                    let mut m = k + 1;
                    while m < v.len() && v[m].depth > level {
                        any |= v[m].checked;
                        m += 1;
                    }
                    v[k].checked = any || v[k].required;
                }
            }
        }
        self.components = WizardComponentVec::from_vec(v);
    }

    /// The validation hook: empty while anything is ticked.
    #[must_use]
    pub fn blocked_reason(&self) -> AzString {
        if self.components.as_ref().iter().any(|c| c.checked) {
            AzString::from_const_str("")
        } else {
            self.reason.clone()
        }
    }
}

page_theme_and_dom!(
    WizardComponentsPage,
    build_components,
    WizardComponentsPage::create(WizardComponentVec::from_const_slice(&[]))
);

/// One component row: the box, the name (and its description), the size;
/// indented by its depth.
fn component_row(
    index: usize,
    c: &WizardComponent,
    on_event: &OptionWizardPageOnEvent,
    theme: Option<UiTheme>,
    look: &DialogKitLook,
) -> Dom {
    let check = check_row(
        &c.label,
        c.checked,
        WizardPageEventKind::ComponentToggled,
        index,
        on_event,
        theme,
        look,
    );
    let mut column = Dom::create_div()
        .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &[]))
        .with_child(check);
    if !c.description.as_str().is_empty() {
        column = column.with_child(dialog_kit::line(
            c.description.clone(),
            &[],
            &look.description,
        ));
    }
    let size = if c.size_bytes == 0 {
        AzString::from_const_str("")
    } else {
        AzString::from(crate::file::DiskSpace::format_bytes(c.size_bytes))
    };
    // The indent is the tree's, the same in every theme.
    #[allow(clippy::cast_possible_wrap)]
    let indent = CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(c.depth as isize * 20),
    ));
    let mut row_skin = look.list_row.clone();
    row_skin.push(indent);
    Dom::create_div()
        .with_ids_and_classes(dialog_kit::class(COMPONENT_ROW_CLASS))
        .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &row_skin))
        .with_accessibility_info(AccessibilityInfo {
            description: if c.required {
                Some(AzString::from_const_str("Required")).into()
            } else {
                None.into()
            },
            ..AccessibilityInfo::named(c.label.clone(), AccessibilityRole::ListItem)
        })
        .with_children(DomVec::from_vec(alloc::vec![
            column,
            dialog_kit::line(size, FIXED_BASE, &look.size),
        ]))
}

fn build_components(page: WizardComponentsPage, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(page.theme);
    let total = page.total_bytes();
    let rows: Vec<Dom> = page
        .components
        .as_ref()
        .iter()
        .enumerate()
        .map(|(i, c)| component_row(i, c, &page.on_event, theme, look))
        .collect();
    let children = alloc::vec![
        dialog_kit::line(page.intro.clone(), &[], &with_block(&look.text, look)),
        Dom::create_div()
            .with_css_props(dialog_kit::part(SCROLL_BOX_BASE, &look.scroll_box))
            .with_accessibility_info(AccessibilityInfo::named(
                "Components",
                AccessibilityRole::List
            ))
            .with_children(DomVec::from_vec(rows)),
        dialog_kit::line(
            AzString::from(alloc::format!(
                "{} {}",
                page.total_label.as_str(),
                crate::file::DiskSpace::format_bytes(total)
            )),
            &[],
            &look.total,
        )
        .with_ids_and_classes(dialog_kit::class(COMPONENTS_TOTAL_CLASS)),
    ];
    page_root(COMPONENTS_CLASS, look, children)
}

// ---- Options ----

/// The options page: checkbox rows, and radio sets (options sharing a
/// group).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WizardOptionsPage {
    /// The options, in order; a radio set's options share a group.
    pub options: WizardOptionVec,
    /// The line over the options ("Select the additional tasks.").
    pub intro: AzString,
    /// Hears `OptionToggled`.
    pub on_event: OptionWizardPageOnEvent,
    /// The widget theme this page is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
}

impl WizardOptionsPage {
    /// An options page over `options`.
    #[must_use]
    pub fn create(options: WizardOptionVec) -> Self {
        Self {
            options,
            intro: AzString::from_const_str("Select the additional tasks Setup should perform."),
            on_event: None.into(),
            theme: OptionUiTheme::None,
        }
    }

    /// The line over the options.
    pub fn set_intro(&mut self, intro: AzString) {
        self.intro = intro;
    }

    /// [`Self::set_intro`] for the builder chain.
    #[must_use]
    pub fn with_intro(mut self, intro: AzString) -> Self {
        self.set_intro(intro);
        self
    }

    /// The callback that hears `OptionToggled`.
    pub fn set_on_event<C: Into<WizardPageOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = on_event_of(data, callback);
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<WizardPageOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Ticks or clears option `index` (a radio clears the rest of its set).
    pub fn choose(&mut self, index: usize, checked: bool) {
        choose_option(&mut self.options, index, checked);
    }
}

page_theme_and_dom!(
    WizardOptionsPage,
    build_options,
    WizardOptionsPage::create(WizardOptionVec::from_const_slice(&[]))
);

fn build_options(page: WizardOptionsPage, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(page.theme);
    let mut children: Vec<Dom> = Vec::new();
    children.push(dialog_kit::line(
        page.intro.clone(),
        &[],
        &with_block(&look.text, look),
    ));
    children.extend(option_rows(&page.options, &page.on_event, theme, look));
    page_root(OPTIONS_CLASS, look, children)
}

// ---- Summary ----

/// The summary page: every choice, read only, key over value.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct WizardSummaryPage {
    /// The entries: "Destination folder" -> "C:\Program Files\AzOffice".
    pub rows: StringPairVec,
    /// The line over the entries ("Setup is ready to install AzOffice.").
    pub intro: AzString,
    /// The widget theme this page is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
}

impl WizardSummaryPage {
    /// A summary of `rows`.
    #[must_use]
    pub fn create(rows: StringPairVec) -> Self {
        Self {
            rows,
            intro: AzString::from_const_str(
                "Setup is ready to install. Review the settings, then click Install.",
            ),
            theme: OptionUiTheme::None,
        }
    }

    /// Appends an entry.
    pub fn add_row(&mut self, key: AzString, value: AzString) {
        let mut v = self.rows.clone().into_library_owned_vec();
        v.push(AzStringPair { key, value });
        self.rows = StringPairVec::from_vec(v);
    }

    /// [`Self::add_row`] for the builder chain.
    #[must_use]
    pub fn with_row(mut self, key: AzString, value: AzString) -> Self {
        self.add_row(key, value);
        self
    }

    /// The line over the entries.
    pub fn set_intro(&mut self, intro: AzString) {
        self.intro = intro;
    }

    /// [`Self::set_intro`] for the builder chain.
    #[must_use]
    pub fn with_intro(mut self, intro: AzString) -> Self {
        self.set_intro(intro);
        self
    }
}

page_theme_and_dom!(
    WizardSummaryPage,
    build_summary,
    WizardSummaryPage::create(StringPairVec::from_const_slice(&[]))
);

fn build_summary(page: WizardSummaryPage, look: &DialogKitLook) -> Dom {
    let entries: Vec<Dom> = page
        .rows
        .as_ref()
        .iter()
        .map(|pair| {
            Dom::create_div()
                .with_ids_and_classes(dialog_kit::class(SUMMARY_ROW_CLASS))
                .with_css_props(dialog_kit::part(COLUMN_BASE, &look.list_row))
                .with_children(DomVec::from_vec(alloc::vec![
                    dialog_kit::line(pair.key.clone(), &[], &look.summary_key),
                    dialog_kit::line(pair.value.clone(), &[], &look.summary_row),
                ]))
        })
        .collect();
    let children = alloc::vec![
        dialog_kit::line(page.intro.clone(), &[], &with_block(&look.text, look)),
        Dom::create_div()
            .with_css_props(dialog_kit::part(SCROLL_BOX_BASE, &look.scroll_box))
            .with_tab_index(TabIndex::Auto)
            .with_accessibility_info(AccessibilityInfo::named(
                "Summary",
                AccessibilityRole::Document
            ))
            .with_children(DomVec::from_vec(entries)),
    ];
    page_root(SUMMARY_CLASS, look, children)
}

// ---- Progress ----

/// The progress page: what is being done, the bar, the current item, a
/// details log behind a button.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WizardProgressPage {
    /// The line over the bar ("Installing AzOffice...").
    pub status: AzString,
    /// The current item ("Copying azword.dll").
    pub current_item: AzString,
    /// The details log, oldest first.
    pub log: StringVec,
    /// The details button while the log is hidden ("Show details").
    pub show_label: AzString,
    /// The details button while the log is shown ("Hide details").
    pub hide_label: AzString,
    /// Hears `DetailsToggled`.
    pub on_event: OptionWizardPageOnEvent,
    /// How far along, 0 to 100.
    pub percent: f32,
    /// The widget theme this page is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
    /// Whether the log is shown.
    pub show_log: bool,
}

impl WizardProgressPage {
    /// A progress page at `percent`.
    #[must_use]
    pub fn create(percent: f32) -> Self {
        Self {
            status: AzString::from_const_str("Installing..."),
            current_item: AzString::from_const_str(""),
            log: StringVec::from_const_slice(&[]),
            show_label: AzString::from_const_str("Show details"),
            hide_label: AzString::from_const_str("Hide details"),
            on_event: None.into(),
            percent,
            theme: OptionUiTheme::None,
            show_log: false,
        }
    }

    /// The line over the bar.
    pub fn set_status(&mut self, status: AzString) {
        self.status = status;
    }

    /// [`Self::set_status`] for the builder chain.
    #[must_use]
    pub fn with_status(mut self, status: AzString) -> Self {
        self.set_status(status);
        self
    }

    /// The current item.
    pub fn set_current_item(&mut self, item: AzString) {
        self.current_item = item;
    }

    /// [`Self::set_current_item`] for the builder chain.
    #[must_use]
    pub fn with_current_item(mut self, item: AzString) -> Self {
        self.set_current_item(item);
        self
    }

    /// The details log.
    pub fn set_log(&mut self, log: StringVec) {
        self.log = log;
    }

    /// [`Self::set_log`] for the builder chain.
    #[must_use]
    pub fn with_log(mut self, log: StringVec) -> Self {
        self.set_log(log);
        self
    }

    /// Whether the log is shown.
    pub const fn set_show_log(&mut self, show: bool) {
        self.show_log = show;
    }

    /// [`Self::set_show_log`] for the builder chain.
    #[must_use]
    pub const fn with_show_log(mut self, show: bool) -> Self {
        self.set_show_log(show);
        self
    }

    /// The details button's two labels.
    pub fn set_details_labels(&mut self, show: AzString, hide: AzString) {
        self.show_label = show;
        self.hide_label = hide;
    }

    /// [`Self::set_details_labels`] for the builder chain.
    #[must_use]
    pub fn with_details_labels(mut self, show: AzString, hide: AzString) -> Self {
        self.set_details_labels(show, hide);
        self
    }

    /// The callback that hears `DetailsToggled`.
    pub fn set_on_event<C: Into<WizardPageOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = on_event_of(data, callback);
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<WizardPageOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Whether the work is done (100 %).
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.percent >= 100.0
    }
}

page_theme_and_dom!(
    WizardProgressPage,
    build_progress,
    WizardProgressPage::create(0.0)
);

fn build_progress(page: WizardProgressPage, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(page.theme);
    let mut children: Vec<Dom> = Vec::new();
    children.push(dialog_kit::line(
        page.status.clone(),
        &[],
        &with_block(&look.text, look),
    ));
    let mut bar = ProgressBar::create(page.percent.clamp(0.0, 100.0))
        .with_accessibility_name(page.status.clone());
    if let Some(t) = theme {
        bar = bar.with_theme(t);
    }
    children.push(
        Dom::create_div()
            .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &look.block))
            .with_children(DomVec::from_vec(alloc::vec![
                Dom::create_div()
                    .with_css_props(dialog_kit::part(GROW_COLUMN_BASE, &[]))
                    .with_child(bar.dom()),
                dialog_kit::line(dialog_kit::percent_text(page.percent), FIXED_BASE, &look.unit),
            ])),
    );
    children.push(
        dialog_kit::line(
            page.current_item.clone(),
            &[],
            &with_block(&look.hint, look),
        )
        .with_ids_and_classes(dialog_kit::class(PROGRESS_ITEM_CLASS)),
    );
    let mut details = Button::create(if page.show_log {
        page.hide_label.clone()
    } else {
        page.show_label.clone()
    })
    .with_on_click(
        RefAny::new(CheckRef {
            on_event: page.on_event.clone(),
            kind: WizardPageEventKind::DetailsToggled,
            index: 0,
            checked: page.show_log,
        }),
        on_details as ButtonOnClickCallbackType,
    );
    if let Some(t) = theme {
        details = details.with_theme(t);
    }
    let details = details.dom().with_accessibility_assign(AccessibilityInfo {
        states: AccessibilityStateVec::from_vec(alloc::vec![if page.show_log {
            AccessibilityState::Expanded
        } else {
            AccessibilityState::Collapsed
        }]),
        ..Default::default()
    });
    children.push(
        Dom::create_div()
            .with_css_props(dialog_kit::part(ROW_MIDDLE_BASE, &look.block))
            .with_child(details),
    );
    if page.show_log {
        let lines: Vec<Dom> = page
            .log
            .as_ref()
            .iter()
            .map(|l| dialog_kit::line(l.clone(), &[], &look.hint))
            .collect();
        children.push(
            Dom::create_div()
                .with_ids_and_classes(dialog_kit::class(PROGRESS_LOG_CLASS))
                .with_css_props(dialog_kit::part(SCROLL_BOX_BASE, &look.scroll_box))
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_info(AccessibilityInfo::named(
                    "Details",
                    AccessibilityRole::List,
                ))
                .with_children(DomVec::from_vec(lines)),
        );
    }
    page_root(PROGRESS_CLASS, look, children)
}

// ---- Finish ----

/// The last page: a title, the text and the actions to run on Finish
/// ("Launch AzOffice now", "Open the readme").
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WizardFinishPage {
    /// The logo glyph, or empty for none.
    pub logo: AzString,
    /// The title ("Completing the AzOffice Setup Wizard").
    pub title: AzString,
    /// The text; paragraphs are `\n\n` apart.
    pub text: AzString,
    /// The actions, as checkbox options.
    pub options: WizardOptionVec,
    /// Hears `OptionToggled`.
    pub on_event: OptionWizardPageOnEvent,
    /// The widget theme this page is PINNED to, or `None` to follow.
    pub theme: OptionUiTheme,
}

impl WizardFinishPage {
    /// A finish page titled `title` saying `text`, with no actions.
    #[must_use]
    pub fn create(title: AzString, text: AzString) -> Self {
        Self {
            logo: AzString::from_const_str(""),
            title,
            text,
            options: WizardOptionVec::from_const_slice(&[]),
            on_event: None.into(),
            theme: OptionUiTheme::None,
        }
    }

    /// The logo glyph.
    pub fn set_logo(&mut self, logo: AzString) {
        self.logo = logo;
    }

    /// [`Self::set_logo`] for the builder chain.
    #[must_use]
    pub fn with_logo(mut self, logo: AzString) -> Self {
        self.set_logo(logo);
        self
    }

    /// The actions.
    pub fn set_options(&mut self, options: WizardOptionVec) {
        self.options = options;
    }

    /// [`Self::set_options`] for the builder chain.
    #[must_use]
    pub fn with_options(mut self, options: WizardOptionVec) -> Self {
        self.set_options(options);
        self
    }

    /// The callback that hears `OptionToggled`.
    pub fn set_on_event<C: Into<WizardPageOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = on_event_of(data, callback);
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<WizardPageOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Ticks or clears action `index`.
    pub fn choose(&mut self, index: usize, checked: bool) {
        choose_option(&mut self.options, index, checked);
    }
}

page_theme_and_dom!(
    WizardFinishPage,
    build_finish,
    WizardFinishPage::create(AzString::from_const_str(""), AzString::from_const_str(""))
);

fn build_finish(page: WizardFinishPage, look: &DialogKitLook) -> Dom {
    let theme = dialog_kit::inner_theme(page.theme);
    let mut children: Vec<Dom> = Vec::new();
    children.extend(logo(&page.logo, look));
    children.push(dialog_kit::line(
        page.title.clone(),
        &[],
        &with_block(&look.heading, look),
    ));
    children.extend(paragraphs(&page.text, &look.text, look));
    children.extend(option_rows(&page.options, &page.on_event, theme, look));
    page_root(FINISH_CLASS, look, children)
}

#[cfg(test)]
mod wizard_pages_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        path_input::PATH_INPUT_CLASS,
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks as tc},
    };

    type Log = Arc<Mutex<Vec<WizardPageEvent>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: WizardPageEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    fn new_log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn data(log: &Log) -> RefAny {
        RefAny::new(log.clone())
    }

    fn texts(node: &Dom) -> Vec<String> {
        fn walk(node: &Dom, out: &mut Vec<String>) {
            if let NodeType::Text(s) = node.root.get_node_type() {
                if !s.as_str().is_empty() {
                    out.push(s.as_ref().as_str().to_string());
                }
            }
            for c in node.children.as_ref() {
                walk(c, out);
            }
        }
        let mut out = Vec::new();
        walk(node, &mut out);
        out
    }

    fn id(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    /// The first node at or above the text `label` that takes a click.
    fn clickable(styled: &StyledDom, label: &str) -> usize {
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let text = nodes
            .iter()
            .position(|n| matches!(n.get_node_type(), NodeType::Text(s) if s.as_str() == label))
            .unwrap_or_else(|| panic!("no text {label:?}"));
        let mut node = NodeId::new(text);
        loop {
            let takes_click = nodes[node.index()]
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click));
            if takes_click {
                return node.index();
            }
            node = hierarchy[node.index()]
                .parent_id()
                .unwrap_or_else(|| panic!("nothing above {label:?} takes a click"));
        }
    }

    fn click(dom: Dom, label: &str) {
        let styled = StyledDom::create_from_dom(dom);
        let target = clickable(&styled, label);
        rv::fire(
            &styled,
            id(target),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .unwrap_or_else(|| panic!("{label:?} takes the click"));
    }

    const MB: u64 = 1024 * 1024;

    fn components() -> WizardComponentVec {
        WizardComponentVec::from_vec(vec![
            WizardComponent::create(AzString::from("Program files"), 300 * MB).with_required(true),
            WizardComponent::create(AzString::from("Applications"), 0),
            WizardComponent::create(AzString::from("Writer"), 120 * MB).with_depth(1),
            WizardComponent::create(AzString::from("Sheets"), 80 * MB).with_depth(1),
            WizardComponent::create(AzString::from("Slides"), 60 * MB).with_depth(1),
            WizardComponent::create(AzString::from("Templates"), 40 * MB)
                .with_description(AzString::from("Letters, invoices, CVs")),
        ])
    }

    fn checked(page: &WizardComponentsPage) -> Vec<bool> {
        page.components.as_ref().iter().map(|c| c.checked).collect()
    }

    fn options() -> WizardOptionVec {
        WizardOptionVec::from_vec(vec![
            WizardOption::create(AzString::from("Create a desktop shortcut"), true),
            WizardOption::create(AzString::from("For me only"), true).with_group(1),
            WizardOption::create(AzString::from("For all users"), false).with_group(1),
            WizardOption::create(AzString::from("Add to the Start menu"), false),
        ])
    }

    #[test]
    fn the_welcome_page_is_its_logo_its_title_and_its_paragraphs() {
        for theme in checks::BOTH {
            let dom = WizardWelcomePage::create(
                AzString::from("Welcome to the AzOffice Setup Wizard"),
                AzString::from("This will install AzOffice.\n\nClose other programs first."),
            )
            .with_logo(AzString::from("install_desktop"))
            .with_theme(theme)
            .dom();
            assert!(tc::has_class(&dom, WELCOME_CLASS), "{}", theme.name());
            assert!(matches!(
                dom.children.as_ref()[0].root.get_node_type(),
                NodeType::Icon(_)
            ));
            assert_eq!(
                texts(&dom),
                vec![
                    "Welcome to the AzOffice Setup Wizard",
                    "This will install AzOffice.",
                    "Close other programs first."
                ],
                "{}",
                theme.name()
            );
        }
    }

    #[test]
    fn the_license_page_holds_next_until_i_accept_is_ticked() {
        let log = new_log();
        let page = WizardLicensePage::create(AzString::from("Grant.\n\nNo warranty."))
            .with_on_event(data(&log), record as WizardPageOnEventCallbackType);
        assert_eq!(
            page.blocked_reason().as_str(),
            "Accept the license agreement to continue."
        );
        assert_eq!(
            page.clone().with_accepted(true).blocked_reason().as_str(),
            ""
        );
        for theme in checks::BOTH {
            let dom = page.clone().with_theme(theme).dom();
            let text_box = tc::find(&dom, LICENSE_TEXT_CLASS).expect("the agreement's box");
            assert_eq!(texts(text_box), vec!["Grant.", "No warranty."]);
            assert_eq!(
                text_box.root.get_tab_index(),
                Some(TabIndex::Auto),
                "the box scrolls by keyboard"
            );
            assert_eq!(
                text_box.root.get_accessibility_info().map(|i| i.role),
                Some(AccessibilityRole::Document)
            );
            assert!(tc::has_focus_ring(text_box, false) && tc::has_focus_ring(text_box, true));
            assert_eq!(tc::find_all(&dom, CHECK_ROW_CLASS).len(), 1);
        }
        click(
            page.clone().with_theme(UiTheme::Flat).dom(),
            "I accept the terms of the license agreement",
        );
        let got = log.lock().expect("log").clone();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, WizardPageEventKind::LicenseAccepted);
        assert!(got[0].checked, "the label ticks the box");
    }

    #[test]
    fn the_destination_page_weighs_the_space_and_warns_when_the_drive_is_short() {
        let short = WizardDestinationPage::create(AzString::from("/opt/AzOffice"), 1024 * MB)
            .with_available(512 * MB);
        assert!(short.is_short());
        assert_eq!(
            short.blocked_reason().as_str(),
            "There is not enough free space on the drive."
        );
        for theme in checks::BOTH {
            let dom = short.clone().with_theme(theme).dom();
            assert!(
                tc::find(&dom, PATH_INPUT_CLASS).is_some(),
                "the field and Browse"
            );
            let space: Vec<String> = tc::find_all(&dom, DESTINATION_SPACE_CLASS)
                .iter()
                .flat_map(|n| texts(n))
                .collect();
            assert_eq!(
                space,
                vec!["Space required: 1.0 GB", "Space available: 512 MB"]
            );
            let warned = tc::nodes(&dom).into_iter().any(|(_, n)| {
                n.root
                    .get_accessibility_info()
                    .is_some_and(|i| i.role == AccessibilityRole::Alert)
            });
            assert!(warned, "{}: a short drive is a warning", theme.name());
        }
        let roomy = short.clone().with_available(2048 * MB);
        assert!(!roomy.is_short());
        assert_eq!(roomy.blocked_reason().as_str(), "");
        let roomy_dom = roomy.with_theme(UiTheme::Flat).dom();
        assert!(!tc::nodes(&roomy_dom).into_iter().any(|(_, n)| {
            n.root
                .get_accessibility_info()
                .is_some_and(|i| i.role == AccessibilityRole::Alert)
        }));
        let unknown = WizardDestinationPage::create(AzString::from("/opt/AzOffice"), 1024 * MB);
        assert!(!unknown.is_short(), "an unknown drive is not short");
        let empty = WizardDestinationPage::create(AzString::from("  "), MB);
        assert_eq!(
            empty.blocked_reason().as_str(),
            "Choose a folder to install into."
        );
    }

    #[test]
    fn a_group_carries_its_parts_a_part_its_group_and_a_required_component_stays() {
        let mut page = WizardComponentsPage::create(components());
        assert_eq!(page.total_bytes(), 600 * MB);
        page.toggle(1, false);
        assert_eq!(checked(&page), vec![true, false, false, false, false, true]);
        assert_eq!(
            page.total_bytes(),
            340 * MB,
            "the parts left with their group"
        );
        page.toggle(3, true);
        assert_eq!(
            checked(&page),
            vec![true, true, false, true, false, true],
            "the part ticks its group"
        );
        page.toggle(3, false);
        assert_eq!(
            checked(&page),
            vec![true, false, false, false, false, true],
            "the last part clears it"
        );
        page.toggle(0, false);
        assert!(checked(&page)[0], "a required component stays");
        assert_eq!(page.blocked_reason().as_str(), "");
        let mut nothing = WizardComponentsPage::create(WizardComponentVec::from_vec(vec![
            WizardComponent::create(AzString::from("Writer"), MB),
        ]));
        nothing.toggle(0, false);
        assert_eq!(
            nothing.blocked_reason().as_str(),
            "Choose at least one component."
        );
        nothing.toggle(9, true);
        assert_eq!(
            checked(&nothing),
            vec![false],
            "an index past the end changes nothing"
        );
    }

    #[test]
    fn the_components_page_lists_the_sizes_and_the_total_follows_a_tick() {
        let log = new_log();
        let page = WizardComponentsPage::create(components())
            .with_on_event(data(&log), record as WizardPageOnEventCallbackType);
        for theme in checks::BOTH {
            let dom = page.clone().with_theme(theme).dom();
            let rows = tc::find_all(&dom, COMPONENT_ROW_CLASS);
            assert_eq!(rows.len(), 6, "{}", theme.name());
            assert_eq!(texts(rows[2]), vec!["Writer", "120 MB"]);
            assert_eq!(
                texts(rows[1]),
                vec!["Applications"],
                "a group shows no size"
            );
            assert_eq!(
                texts(rows[5]),
                vec!["Templates", "Letters, invoices, CVs", "40 MB"]
            );
            let total = tc::find(&dom, COMPONENTS_TOTAL_CLASS).expect("the total");
            assert_eq!(texts(total), vec!["Space required: 600 MB"]);
        }
        let mut fewer = page.clone();
        fewer.toggle(2, false);
        let dom = fewer.with_theme(UiTheme::Flat).dom();
        assert_eq!(
            texts(tc::find(&dom, COMPONENTS_TOTAL_CLASS).expect("the total")),
            vec!["Space required: 480 MB"]
        );
        click(page.with_theme(UiTheme::Flat).dom(), "Sheets");
        let got = log.lock().expect("log").clone();
        assert_eq!(
            (got[0].kind, got[0].index, got[0].checked),
            (WizardPageEventKind::ComponentToggled, 3, false)
        );
    }

    #[test]
    fn a_radio_set_keeps_one_option_and_a_checkbox_flips_alone() {
        let mut page = WizardOptionsPage::create(options());
        let ticked = |p: &WizardOptionsPage| -> Vec<bool> {
            p.options.as_ref().iter().map(|o| o.checked).collect()
        };
        page.choose(2, true);
        assert_eq!(ticked(&page), vec![true, false, true, false]);
        page.choose(2, false);
        assert_eq!(
            ticked(&page),
            vec![true, false, true, false],
            "a radio is not cleared alone"
        );
        page.choose(3, true);
        page.choose(0, false);
        assert_eq!(ticked(&page), vec![false, false, true, true]);

        let log = new_log();
        let page = WizardOptionsPage::create(options())
            .with_on_event(data(&log), record as WizardPageOnEventCallbackType);
        for theme in checks::BOTH {
            let dom = page.clone().with_theme(theme).dom();
            assert_eq!(
                tc::find_all(&dom, CHECK_ROW_CLASS).len(),
                2,
                "two checkboxes"
            );
            let all = texts(&dom);
            for label in ["For me only", "For all users"] {
                assert!(
                    all.iter().any(|t| t == label),
                    "{}: the radio set shows {label}",
                    theme.name()
                );
            }
        }
        click(
            page.with_theme(UiTheme::Flat).dom(),
            "Create a desktop shortcut",
        );
        let got = log.lock().expect("log").clone();
        assert_eq!(
            (got[0].kind, got[0].index, got[0].checked),
            (WizardPageEventKind::OptionToggled, 0, false)
        );
    }

    #[test]
    fn the_summary_page_reads_key_over_value() {
        let dom = WizardSummaryPage::create(StringPairVec::from_const_slice(&[]))
            .with_intro(AzString::from("Ready."))
            .with_row(
                AzString::from("Destination folder"),
                AzString::from("/opt/AzOffice"),
            )
            .with_row(
                AzString::from("Components"),
                AzString::from("Writer, Sheets"),
            )
            .with_theme(UiTheme::Flora)
            .dom();
        assert_eq!(
            texts(&dom),
            vec![
                "Ready.",
                "Destination folder",
                "/opt/AzOffice",
                "Components",
                "Writer, Sheets"
            ]
        );
        assert_eq!(tc::find_all(&dom, SUMMARY_ROW_CLASS).len(), 2);
    }

    #[test]
    fn the_progress_page_shows_the_bar_the_item_and_the_details_behind_a_button() {
        assert!(!WizardProgressPage::create(42.0).is_done());
        assert!(WizardProgressPage::create(100.0).is_done());
        let log = new_log();
        let page = WizardProgressPage::create(42.0)
            .with_status(AzString::from("Installing AzOffice..."))
            .with_current_item(AzString::from("Copying azword.dll"))
            .with_log(StringVec::from_vec(vec![
                AzString::from("Created /opt/AzOffice"),
                AzString::from("Copied azcore.dll"),
            ]))
            .with_on_event(data(&log), record as WizardPageOnEventCallbackType);
        for theme in checks::BOTH {
            let dom = page.clone().with_theme(theme).dom();
            let all = texts(&dom);
            for want in [
                "Installing AzOffice...",
                "42 %",
                "Copying azword.dll",
                "Show details",
            ] {
                assert!(
                    all.iter().any(|t| t == want),
                    "{}: {want} in {all:?}",
                    theme.name()
                );
            }
            assert!(
                tc::find(&dom, PROGRESS_LOG_CLASS).is_none(),
                "the log starts hidden"
            );
            // The bar is the ProgressBar widget: its role sits on the tree
            // its VirtualView renders at layout time, so the page's own DOM
            // carries the mount.
            let bar = tc::nodes(&dom).into_iter().any(|(_, n)| {
                matches!(n.root.get_node_type(), azul_core::dom::NodeType::VirtualView)
            });
            assert!(bar, "{}: a progress bar", theme.name());
            let shown = page.clone().with_show_log(true).with_theme(theme).dom();
            let log_box = tc::find(&shown, PROGRESS_LOG_CLASS).expect("the log");
            assert_eq!(
                texts(log_box),
                vec!["Created /opt/AzOffice", "Copied azcore.dll"]
            );
            assert!(texts(&shown).iter().any(|t| t == "Hide details"));
        }
        click(page.with_theme(UiTheme::Flat).dom(), "Show details");
        let got = log.lock().expect("log").clone();
        assert_eq!(
            (got[0].kind, got[0].checked),
            (WizardPageEventKind::DetailsToggled, true)
        );
    }

    #[test]
    fn the_finish_page_offers_its_actions_as_checkboxes() {
        let log = new_log();
        let page = WizardFinishPage::create(
            AzString::from("Completing the AzOffice Setup Wizard"),
            AzString::from("Setup has installed AzOffice."),
        )
        .with_options(WizardOptionVec::from_vec(vec![
            WizardOption::create(AzString::from("Launch AzOffice now"), true),
            WizardOption::create(AzString::from("Open the readme"), false),
        ]))
        .with_on_event(data(&log), record as WizardPageOnEventCallbackType);
        let dom = page.clone().with_theme(UiTheme::Flat).dom();
        assert_eq!(
            texts(&dom),
            vec![
                "Completing the AzOffice Setup Wizard",
                "Setup has installed AzOffice.",
                "Launch AzOffice now",
                "Open the readme"
            ]
        );
        click(dom, "Open the readme");
        let got = log.lock().expect("log").clone();
        assert_eq!(
            (got[0].kind, got[0].index, got[0].checked),
            (WizardPageEventKind::OptionToggled, 1, true)
        );
        let mut p = page;
        p.choose(1, true);
        assert!(p.options.as_ref()[1].checked);
    }

    /// `page` pinned to `theme` when it is set.
    fn pin<T>(mut page: T, theme: OptionUiTheme, set: fn(&mut T, UiTheme)) -> T {
        if let OptionUiTheme::Some(t) = theme {
            set(&mut page, t);
        }
        page
    }

    fn welcome(t: OptionUiTheme) -> Dom {
        pin(
            WizardWelcomePage::create(AzString::from("Welcome"), AzString::from("Hello."))
                .with_logo(AzString::from("install_desktop")),
            t,
            WizardWelcomePage::set_theme,
        )
        .dom()
    }

    fn license(t: OptionUiTheme) -> Dom {
        pin(
            WizardLicensePage::create(AzString::from("Grant.")),
            t,
            WizardLicensePage::set_theme,
        )
        .dom()
    }

    fn destination(t: OptionUiTheme) -> Dom {
        pin(
            WizardDestinationPage::create(AzString::from("/opt/AzOffice"), 1024 * MB)
                .with_available(10 * MB),
            t,
            WizardDestinationPage::set_theme,
        )
        .dom()
    }

    fn components_page(t: OptionUiTheme) -> Dom {
        pin(
            WizardComponentsPage::create(components()),
            t,
            WizardComponentsPage::set_theme,
        )
        .dom()
    }

    fn options_page(t: OptionUiTheme) -> Dom {
        pin(
            WizardOptionsPage::create(options()),
            t,
            WizardOptionsPage::set_theme,
        )
        .dom()
    }

    fn summary(t: OptionUiTheme) -> Dom {
        pin(
            WizardSummaryPage::create(StringPairVec::from_const_slice(&[]))
                .with_row(AzString::from("Folder"), AzString::from("/opt")),
            t,
            WizardSummaryPage::set_theme,
        )
        .dom()
    }

    fn progress(t: OptionUiTheme) -> Dom {
        pin(
            WizardProgressPage::create(30.0)
                .with_log(StringVec::from_vec(vec![AzString::from("Copied a")]))
                .with_show_log(true),
            t,
            WizardProgressPage::set_theme,
        )
        .dom()
    }

    fn finish(t: OptionUiTheme) -> Dom {
        pin(
            WizardFinishPage::create(AzString::from("Done"), AzString::from("Installed."))
                .with_options(options()),
            t,
            WizardFinishPage::set_theme,
        )
        .dom()
    }

    type PageFn = fn(OptionUiTheme) -> Dom;

    /// Every page, with content, for the theme checks.
    fn every_page() -> Vec<(&'static str, PageFn)> {
        vec![
            ("wizard welcome page", welcome as PageFn),
            ("wizard license page", license as PageFn),
            ("wizard destination page", destination as PageFn),
            ("wizard components page", components_page as PageFn),
            ("wizard options page", options_page as PageFn),
            ("wizard summary page", summary as PageFn),
            ("wizard progress page", progress as PageFn),
            ("wizard finish page", finish as PageFn),
        ]
    }

    /// Whether `node` is one the pages built (by its classes).
    fn own(node: &Dom) -> bool {
        node.root.get_ids_and_classes().as_ref().iter().any(|c| {
            matches!(c, azul_core::dom::IdOrClass::Class(s)
                if s.as_str().starts_with("__azul-native-wizard-")
                    || s.as_str().starts_with("__azul-native-dialog-kit"))
        })
    }

    #[test]
    fn every_page_follows_the_app_theme_and_rings_its_own_stops() {
        for (name, page) in every_page() {
            checks::assert_follows_the_app_theme(
                name,
                || page(OptionUiTheme::None),
                |t: UiTheme| page(OptionUiTheme::Some(t)),
            );
            for theme in checks::BOTH {
                let dom = page(OptionUiTheme::Some(theme));
                for (path, node) in tc::focusable(&dom) {
                    if own(node) {
                        assert!(
                            tc::has_focus_ring(node, false),
                            "{name}: {path} has no light ring"
                        );
                        assert!(
                            tc::has_focus_ring(node, true),
                            "{name}: {path} has no dark ring"
                        );
                    }
                }
                // The structure of the pages' own nodes is declared once,
                // outside the theme blocks (the nested widgets answer for
                // themselves in their own suites).
                let followed = checks::under(theme, || page(OptionUiTheme::None));
                let own_paths: Vec<String> = tc::nodes(&followed)
                    .into_iter()
                    .filter(|(_, n)| own(n))
                    .map(|(p, _)| p)
                    .collect();
                let themed: Vec<String> = tc::themed_structure(&followed, &[])
                    .into_iter()
                    .filter(|m| {
                        own_paths
                            .iter()
                            .any(|p| m.split([':', ' ']).next() == Some(p.as_str()))
                    })
                    .collect();
                assert!(
                    themed.is_empty(),
                    "{name} ({}): {}",
                    theme.name(),
                    themed.join("\n")
                );
            }
        }
    }
}
