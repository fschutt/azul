//! Wizard layout widget - the frame of a multi-page dialog such as "Add
//! account": the steps rail across the top (the [`Stepper`] widget), the
//! current page under it with its title, and the button row at the bottom:
//! Cancel at the left, Back and Next at the right, Finish in place of Next
//! on the last step. The Backstage's account wizard.
//!
//! The layout owns nothing: the app keeps the current step and the page's
//! content (a `Dom` it hands in), hears every button and a click on the
//! rail through ONE callback ([`WizardLayout::on_event`], a [`WizardEvent`]
//! naming what was asked), and rebuilds. A Back on the first step and a
//! Next the app forbade ([`WizardLayout::can_go_next`]) are disabled: they
//! keep their Tab stop, run nothing and say why on hover and on keyboard
//! focus (the Button's own disabled state). For
//! assistive technology the layout is a group named "<title>: step i of n,
//! <step>"; the buttons are named by their labels.
//!
//! Three frames ([`WizardLayoutStyle`]): the steps RAIL over the page (the
//! default, an Office dialog), the BANNER of a Windows installer (the step's
//! title and a subtitle on a white band, a glyph at its right, no rail) and
//! the SIDE PANEL of the macOS installer (the steps listed down the left,
//! the current one marked). An app may switch per page - Wizard97 shows the
//! welcome and finish pages without the banner. The standard sizes
//! ([`WizardLayoutSize`]) fix the frame at a classic installer's size;
//! `Fill` takes the host's.
//!
//! A page's validation hook is [`WizardLayout::set_validation`]: a reason
//! ("Accept the license agreement to continue.") holds Next - the button is
//! disabled, dimmed, announced unavailable and described by the reason,
//! which it shows on hover and on keyboard focus and the button row also
//! shows. [`WizardLayout::can_go_back`] holds Back the same way (an
//! installer's progress and finish pages). A button held without the app's
//! reason gives the layout's own ("Complete this page to continue.").
//!
//! Key types: [`WizardLayout`], [`WizardEvent`], [`WizardEventKind`],
//! [`WizardLayoutStyle`], [`WizardLayoutSize`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec, OptionDom},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutHeight, LayoutMinHeight, LayoutMinWidth, LayoutWidth,
        },
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{ButtonOnClickCallbackType, ButtonType},
        stepper::{Stepper, StepperOnStepChangeCallbackType, StepperState},
    },
};

static LAYOUT_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-wizard-layout"))];
static RAIL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-wizard-layout-rail",
))];
static PAGE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-wizard-layout-page",
))];
static TITLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-wizard-layout-title",
))];
static BUTTONS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-wizard-layout-buttons",
))];
static SPACER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-wizard-layout-spacer",
))];

/// The banner's class (a Windows installer's band over the page).
pub const BANNER_CLASS: &str = "__azul-native-wizard-layout-banner";
/// The class of the row that holds the side panel beside the page.
pub const BODY_CLASS: &str = "__azul-native-wizard-layout-body";
/// The side panel's class (the macOS installer's step list).
pub const SIDE_PANEL_CLASS: &str = "__azul-native-wizard-layout-side-panel";
/// The class of one step of the side panel.
pub const SIDE_STEP_CLASS: &str = "__azul-native-wizard-layout-side-step";
/// The class of the reason a held Next shows in the button row.
pub const REASON_CLASS: &str = "__azul-native-wizard-layout-reason";
/// The class of a button's box in the button row.
const BUTTON_BOX_CLASS: &str = "__azul-native-wizard-layout-button";

/// Why Next (or Finish) is held when the app gave no reason.
const NEXT_HELD_REASON: &str = "Complete this page to continue.";
/// Why Back is held on the first step.
const FIRST_STEP_REASON: &str = "This is the first step.";
/// Why Back is held when the app holds it (`can_go_back` unset).
const BACK_HELD_REASON: &str = "You cannot go back from this step.";

/// What the user asked for.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WizardEventKind {
    /// Back: go to `step - 1`.
    Back,
    /// Next: go to `step + 1` (the app validates the page first).
    Next,
    /// Finish, on the last step.
    Finish,
    /// Cancel.
    Cancel,
    /// A step on the rail was clicked: `step` is that step; the app decides
    /// whether it may be jumped to.
    Step,
}

/// One request from the wizard's chrome: what, from (or to) which step.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WizardEvent {
    /// The current step (`Back`, `Next`, `Finish`, `Cancel`) or the step
    /// clicked on the rail (`Step`).
    pub step: usize,
    /// What was asked.
    pub kind: WizardEventKind,
}

/// Callback invoked for a request from the wizard's chrome.
pub type WizardOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, WizardEvent) -> Update;
impl_widget_callback!(
    WizardOnEvent,
    OptionWizardOnEvent,
    WizardOnEventCallback,
    WizardOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        WizardOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: WIZARD_ON_EVENT_INVOKER,
    invoker_ty:     AzWizardOnEventCallbackInvoker,
    thunk_fn:       az_wizard_on_event_callback_thunk,
    setter_fn:      AzApp_setWizardOnEventCallbackInvoker,
    from_handle_fn: AzWizardOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzWizardOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: WizardEvent ],
}

/// How a wizard frames its page.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum WizardLayoutStyle {
    /// The steps rail over the page (an Office dialog).
    #[default]
    Rail,
    /// A Windows installer: a banner with the step's title, the subtitle
    /// and the glyph over the page; no rail.
    Banner,
    /// The macOS installer: the steps listed down a side panel at the left,
    /// the current one marked, the glyph over them.
    SidePanel,
}

/// The size a wizard's frame takes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum WizardLayoutSize {
    /// The host's size (a window the app sized, a backstage page).
    #[default]
    Fill,
    /// 500 x 380: the NSIS / Inno Setup installer.
    Compact,
    /// 640 x 480: the classic wizard.
    Classic,
    /// 620 x 440: the macOS installer.
    MacInstaller,
    /// 800 x 600: a wizard with room for a list.
    Large,
}

impl WizardLayoutSize {
    /// The width in logical pixels (0 for `Fill`).
    #[must_use]
    pub const fn width(self) -> f32 {
        match self {
            Self::Fill => 0.0,
            Self::Compact => 500.0,
            Self::Classic => 640.0,
            Self::MacInstaller => 620.0,
            Self::Large => 800.0,
        }
    }

    /// The height in logical pixels (0 for `Fill`).
    #[must_use]
    pub const fn height(self) -> f32 {
        match self {
            Self::Fill => 0.0,
            Self::Compact => 380.0,
            Self::Classic => 480.0,
            Self::MacInstaller => 440.0,
            Self::Large => 600.0,
        }
    }
}

/// A wizard: the steps rail, the current page and the Back / Next / Finish
/// buttons.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WizardLayout {
    /// The steps' labels, in order; `steps.len()` is the step count.
    pub steps: StringVec,
    /// The current page's content, or `None` for an empty page.
    pub page: OptionDom,
    /// The wizard's title ("Add account"), over the page.
    pub title: AzString,
    /// The Back button's label.
    pub back_label: AzString,
    /// The Next button's label.
    pub next_label: AzString,
    /// The Finish button's label (on the last step, in Next's place).
    pub finish_label: AzString,
    /// The Cancel button's label, or empty for no Cancel button.
    pub cancel_label: AzString,
    /// Hears every button and the rail.
    pub on_event: OptionWizardOnEvent,
    /// The current step, `0..steps.len()`.
    pub current_step: usize,
    /// The line under the title (the banner's description of the step), or
    /// empty for none.
    pub subtitle: AzString,
    /// The glyph of the banner (at its right) or of the side panel (over
    /// the steps), a `Dom::create_icon` name, or empty for none.
    pub icon: AzString,
    /// Why Next is held ("Accept the license agreement to continue."), or
    /// empty: a reason holds Next and the button row shows it.
    pub blocked_reason: AzString,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// How the page is framed: rail, banner or side panel.
    pub style: WizardLayoutStyle,
    /// The frame's size: the host's, or a standard installer size.
    pub size: WizardLayoutSize,
    /// Whether Next (or Finish) does anything: unset while the page is not
    /// valid yet, the button is disabled and says why.
    pub can_go_next: bool,
    /// Whether Back does anything (unset on an installer's progress and
    /// finish pages); Back on the first step never does.
    pub can_go_back: bool,
}

/// What a theme decides about a wizard layout: the SKIN of each part, laid
/// over the part's base (the layout's structure, the same in every theme:
/// `WIZARD_LAYOUT_*_BASE`) by [`build`].
pub(crate) struct WizardLayoutLook {
    /// The layout.
    pub layout: Vec<CssPropertyWithConditions>,
    /// The box around the rail.
    pub rail: Vec<CssPropertyWithConditions>,
    /// The page.
    pub page: Vec<CssPropertyWithConditions>,
    /// The title.
    pub title: Vec<CssPropertyWithConditions>,
    /// The button row.
    pub buttons: Vec<CssPropertyWithConditions>,
    /// The box around one button (its spacing).
    pub button: Vec<CssPropertyWithConditions>,
    /// The subtitle under the title.
    pub subtitle: Vec<CssPropertyWithConditions>,
    /// The banner (a Windows installer's white band over the page).
    pub banner: Vec<CssPropertyWithConditions>,
    /// The banner's title (the step).
    pub banner_title: Vec<CssPropertyWithConditions>,
    /// The banner's glyph, at its right.
    pub banner_icon: Vec<CssPropertyWithConditions>,
    /// The side panel (the macOS installer's step list).
    pub side_panel: Vec<CssPropertyWithConditions>,
    /// The side panel's glyph, over the steps.
    pub side_icon: Vec<CssPropertyWithConditions>,
    /// One step of the side panel.
    pub side_step: Vec<CssPropertyWithConditions>,
    /// Added to the current step of the side panel.
    pub side_step_current: Vec<CssPropertyWithConditions>,
    /// The reason Next is held, in the button row. (A held button is
    /// dimmed by the Button alone: no box skin of its own.)
    pub reason: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the layout, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the layout's structure, in every theme ----

/// The layout: a column that takes its space, the page growing in it.
pub(crate) static WIZARD_LAYOUT_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A band of the layout (the rail, the button row): one row on its
/// midline, never growing, its text never selected by a drag.
pub(crate) static WIZARD_LAYOUT_BAND_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The page: the rest of the layout, a column of the title and the content.
pub(crate) static WIZARD_LAYOUT_PAGE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
];

/// The spacer between Cancel and the right-hand buttons.
pub(crate) static WIZARD_LAYOUT_SPACER_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

/// A button's box keeps its size.
pub(crate) static WIZARD_LAYOUT_BUTTON_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

impl WizardLayout {
    /// A wizard titled `title` over `steps`, on the first step, with an
    /// empty page, "Back" / "Next" / "Finish" / "Cancel" buttons and Next
    /// enabled.
    #[must_use]
    pub fn create(title: AzString, steps: StringVec) -> Self {
        Self {
            steps,
            page: OptionDom::None,
            title,
            back_label: AzString::from_const_str("Back"),
            next_label: AzString::from_const_str("Next"),
            finish_label: AzString::from_const_str("Finish"),
            cancel_label: AzString::from_const_str("Cancel"),
            on_event: None.into(),
            current_step: 0,
            subtitle: AzString::from_const_str(""),
            icon: AzString::from_const_str(""),
            blocked_reason: AzString::from_const_str(""),
            theme: crate::widgets::themes::OptionUiTheme::None,
            style: WizardLayoutStyle::Rail,
            size: WizardLayoutSize::Fill,
            can_go_next: true,
            can_go_back: true,
        }
    }

    /// The current page's content.
    pub fn set_page(&mut self, page: Dom) {
        self.page = OptionDom::Some(page);
    }

    /// [`Self::set_page`] for the builder chain.
    #[must_use]
    pub fn with_page(mut self, page: Dom) -> Self {
        self.set_page(page);
        self
    }

    /// The current step.
    pub const fn set_current_step(&mut self, step: usize) {
        self.current_step = step;
    }

    /// [`Self::set_current_step`] for the builder chain.
    #[must_use]
    pub const fn with_current_step(mut self, step: usize) -> Self {
        self.set_current_step(step);
        self
    }

    /// The buttons' labels (an empty Cancel label hides Cancel).
    pub fn set_labels(&mut self, back: AzString, next: AzString, finish: AzString, cancel: AzString) {
        self.back_label = back;
        self.next_label = next;
        self.finish_label = finish;
        self.cancel_label = cancel;
    }

    /// [`Self::set_labels`] for the builder chain.
    #[must_use]
    pub fn with_labels(
        mut self,
        back: AzString,
        next: AzString,
        finish: AzString,
        cancel: AzString,
    ) -> Self {
        self.set_labels(back, next, finish, cancel);
        self
    }

    /// Whether Next (or Finish) does anything.
    pub const fn set_can_go_next(&mut self, can_go_next: bool) {
        self.can_go_next = can_go_next;
    }

    /// [`Self::set_can_go_next`] for the builder chain.
    #[must_use]
    pub const fn with_can_go_next(mut self, can_go_next: bool) -> Self {
        self.set_can_go_next(can_go_next);
        self
    }

    /// Whether Back does anything (Back on the first step never does).
    pub const fn set_can_go_back(&mut self, can_go_back: bool) {
        self.can_go_back = can_go_back;
    }

    /// [`Self::set_can_go_back`] for the builder chain.
    #[must_use]
    pub const fn with_can_go_back(mut self, can_go_back: bool) -> Self {
        self.set_can_go_back(can_go_back);
        self
    }

    /// The page's validation hook: `reason` empty lets Next go; a reason
    /// holds Next, and the button row says why.
    pub fn set_validation(&mut self, reason: AzString) {
        self.can_go_next = reason.as_str().is_empty();
        self.blocked_reason = reason;
    }

    /// [`Self::set_validation`] for the builder chain.
    #[must_use]
    pub fn with_validation(mut self, reason: AzString) -> Self {
        self.set_validation(reason);
        self
    }

    /// Whether Next (or Finish) is held: unset `can_go_next` or a reason.
    #[must_use]
    pub fn is_next_held(&self) -> bool {
        !self.can_go_next || !self.blocked_reason.as_str().is_empty()
    }

    /// The line under the title (the banner's description of the step).
    pub fn set_subtitle(&mut self, subtitle: AzString) {
        self.subtitle = subtitle;
    }

    /// [`Self::set_subtitle`] for the builder chain.
    #[must_use]
    pub fn with_subtitle(mut self, subtitle: AzString) -> Self {
        self.set_subtitle(subtitle);
        self
    }

    /// The glyph of the banner or the side panel (a `Dom::create_icon`
    /// name).
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// How the page is framed.
    pub const fn set_style(&mut self, style: WizardLayoutStyle) {
        self.style = style;
    }

    /// [`Self::set_style`] for the builder chain.
    #[must_use]
    pub const fn with_style(mut self, style: WizardLayoutStyle) -> Self {
        self.set_style(style);
        self
    }

    /// The frame's size.
    pub const fn set_size(&mut self, size: WizardLayoutSize) {
        self.size = size;
    }

    /// [`Self::set_size`] for the builder chain.
    #[must_use]
    pub const fn with_size(mut self, size: WizardLayoutSize) -> Self {
        self.set_size(size);
        self
    }

    /// Pin the widget theme; unset, the layout follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The callback that hears every button and the rail.
    pub fn set_on_event<C: Into<WizardOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_event = Some(WizardOnEvent {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<WizardOnEventCallback>>(mut self, data: RefAny, cb: C) -> Self {
        self.set_on_event(data, cb);
        self
    }

    /// Whether the current step is the last one (Finish in Next's place).
    #[must_use]
    pub fn is_last_step(&self) -> bool {
        self.current_step + 1 >= self.steps.as_ref().len()
    }

    /// Replaces `self` with an empty wizard and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(
            AzString::from_const_str(""),
            StringVec::from_const_slice(&[]),
        );
        core::mem::swap(&mut s, self);
        s
    }

    /// The layout's DOM. The look comes from the theme module
    /// (`themes::flat::wizard_layout` / `themes::flora::wizard_layout`);
    /// `None` carries both looks, each in its `@theme(<name>)` block, and
    /// the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::wizard_layout(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::wizard_layout(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::wizard_layout,
                crate::widgets::themes::flora::wizard_layout,
            ),
        }
    }
}

impl Default for WizardLayout {
    fn default() -> Self {
        Self::create(
            AzString::from_const_str(""),
            StringVec::from_const_slice(&[]),
        )
    }
}

impl From<WizardLayout> for Dom {
    fn from(w: WizardLayout) -> Self {
        w.dom()
    }
}

/// What every button of one wizard shares: the app's callback and the step.
struct WizardShared {
    on_event: OptionWizardOnEvent,
    step: usize,
}

/// Hands `kind` at `step` to the app's callback.
fn emit(data: &mut RefAny, info: CallbackInfo, kind: WizardEventKind, step: usize) -> Update {
    let Some(shared) = data.downcast_ref::<WizardShared>() else {
        return Update::DoNothing;
    };
    match shared.on_event.as_ref() {
        Some(WizardOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, WizardEvent { step, kind })
        }
        None => Update::DoNothing,
    }
}

/// The current step of the wizard whose shared state is `data`.
fn step_of(data: &mut RefAny) -> usize {
    data.downcast_ref::<WizardShared>().map_or(0, |s| s.step)
}

extern "C" fn on_back(mut data: RefAny, info: CallbackInfo) -> Update {
    let step = step_of(&mut data);
    emit(&mut data, info, WizardEventKind::Back, step)
}

extern "C" fn on_next(mut data: RefAny, info: CallbackInfo) -> Update {
    let step = step_of(&mut data);
    emit(&mut data, info, WizardEventKind::Next, step)
}

extern "C" fn on_finish(mut data: RefAny, info: CallbackInfo) -> Update {
    let step = step_of(&mut data);
    emit(&mut data, info, WizardEventKind::Finish, step)
}

extern "C" fn on_cancel(mut data: RefAny, info: CallbackInfo) -> Update {
    let step = step_of(&mut data);
    emit(&mut data, info, WizardEventKind::Cancel, step)
}

/// A step on the rail was clicked.
extern "C" fn on_rail_step(mut data: RefAny, info: CallbackInfo, state: StepperState) -> Update {
    emit(&mut data, info, WizardEventKind::Step, state.current_step)
}

/// The layout's DOM in `look`. Rail: layout [rail [stepper], page [title,
/// subtitle?, content?], buttons]. Banner: layout [banner [text [step,
/// subtitle?], glyph?], page [content?], buttons]. Side panel: layout [body
/// [side panel [glyph?, steps], page [title, subtitle?, content?]],
/// buttons]. Buttons: [cancel?, reason | spacer, back, next | finish]; a
/// held button is the Button's own disabled state with its reason
/// (`dialog_kit::row_button`): it keeps its Tab stop, runs nothing and says
/// why on hover and on keyboard focus. Every part is its base (the
/// structure), then the look's skin; the rail and the buttons are the
/// toolkit's own widgets, pinned to the layout's theme (or following the
/// app theme with it).
pub(crate) fn build(wizard: WizardLayout, look: &WizardLayoutLook) -> Dom {
    use azul_core::a11y::{
        AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec,
    };
    use crate::widgets::themes::theme_blocks::stack_parts;

    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let last = wizard.is_last_step();
    let next_held = wizard.is_next_held();
    let WizardLayout {
        steps,
        page,
        title,
        back_label,
        next_label,
        finish_label,
        cancel_label,
        on_event,
        current_step,
        subtitle,
        icon,
        blocked_reason,
        theme,
        style,
        size,
        can_go_next: _,
        can_go_back,
    } = wizard;
    let theme = theme.into_option();
    let count = steps.as_ref().len();
    let step_label = steps
        .as_ref()
        .get(current_step)
        .map_or_else(|| AzString::from_const_str(""), Clone::clone);
    let shared = RefAny::new(WizardShared {
        on_event,
        step: current_step,
    });
    let text = |s: AzString, base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        crate::widgets::widget_p_with_text(s).with_css_props(part(base, skin))
    };

    // The page: the title and the subtitle over the app's content - in the
    // banner's frame the banner says them instead.
    let mut page_dom = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(PAGE_CLASS))
        .with_css_props(part(WIZARD_LAYOUT_PAGE_BASE, &look.page));
    if style != WizardLayoutStyle::Banner {
        page_dom = page_dom.with_child(
            text(title.clone(), &[], &look.title)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TITLE_CLASS)),
        );
        if !subtitle.as_str().is_empty() {
            page_dom = page_dom.with_child(text(subtitle.clone(), &[], &look.subtitle));
        }
    }
    if let Some(content) = page.into_option() {
        page_dom = page_dom.with_child(content);
    }

    // The frame over (or beside) the page.
    let glyph = |skin: &[CssPropertyWithConditions]| {
        (!icon.as_str().is_empty()).then(|| {
            Dom::create_icon(icon.clone()).with_css_props(part(WIZARD_LAYOUT_FIXED_BASE, skin))
        })
    };
    let header: Option<Dom> = match style {
        WizardLayoutStyle::Rail => {
            // The rail: the stepper, a click on a step asking the app.
            let on_step: StepperOnStepChangeCallbackType = on_rail_step;
            let mut rail = Stepper::create(steps.clone())
                .with_current_step(current_step)
                .with_on_step_change(shared.clone(), on_step);
            if let Some(theme) = theme {
                rail = rail.with_theme(theme);
            }
            Some(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(RAIL_CLASS))
                    .with_css_props(part(WIZARD_LAYOUT_BAND_BASE, &look.rail))
                    .with_child(rail.dom()),
            )
        }
        WizardLayoutStyle::Banner => {
            let mut words: Vec<Dom> = alloc::vec![text(step_label.clone(), &[], &look.banner_title)];
            if !subtitle.as_str().is_empty() {
                words.push(text(subtitle, &[], &look.subtitle));
            }
            let mut band: Vec<Dom> = alloc::vec![Dom::create_div()
                .with_css_props(part(WIZARD_LAYOUT_GROW_COLUMN_BASE, &[]))
                .with_children(DomVec::from_vec(words))];
            band.extend(glyph(&look.banner_icon));
            Some(
                Dom::create_div()
                    .with_ids_and_classes(classes_of(BANNER_CLASS))
                    .with_css_props(part(WIZARD_LAYOUT_BAND_BASE, &look.banner))
                    .with_children(DomVec::from_vec(band)),
            )
        }
        WizardLayoutStyle::SidePanel => None,
    };

    // The buttons: a button in its box, with its click (`on_click`) or -
    // held - disabled with `reason`: the Button's own disabled state (it
    // keeps its Tab stop and says why on hover and on keyboard focus).
    let button = |label: AzString,
                  kind: ButtonType,
                  on_click: Option<ButtonOnClickCallbackType>,
                  reason: AzString| {
        use crate::widgets::dialog_kit::RowAction;
        crate::widgets::dialog_kit::row_button(
            label,
            kind,
            match on_click {
                Some(cb) => RowAction::Click(shared.clone(), cb),
                None => RowAction::Disabled(reason),
            },
            theme,
            BUTTON_BOX_CLASS,
            WIZARD_LAYOUT_BUTTON_BASE,
            &look.button,
        )
    };
    let mut buttons: Vec<Dom> = Vec::with_capacity(5);
    if !cancel_label.as_str().is_empty() {
        buttons.push(button(
            cancel_label,
            ButtonType::Default,
            Some(on_cancel),
            AzString::from_const_str(""),
        ));
    }
    if next_held && !blocked_reason.as_str().is_empty() {
        // The reason takes the spacer's place, beside the held button.
        buttons.push(
            text(blocked_reason.clone(), WIZARD_LAYOUT_REASON_BASE, &look.reason)
                .with_ids_and_classes(classes_of(REASON_CLASS)),
        );
    } else {
        buttons.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(SPACER_CLASS))
                .with_css_props(part(WIZARD_LAYOUT_SPACER_BASE, &[])),
        );
    }
    let back: ButtonOnClickCallbackType = on_back;
    buttons.push(button(
        back_label,
        ButtonType::Default,
        (current_step > 0 && can_go_back).then_some(back),
        AzString::from_const_str(if current_step == 0 {
            FIRST_STEP_REASON
        } else {
            BACK_HELD_REASON
        }),
    ));
    let finish: ButtonOnClickCallbackType = on_finish;
    let next: ButtonOnClickCallbackType = on_next;
    let (forward_label, forward) = if last {
        (finish_label, finish)
    } else {
        (next_label, next)
    };
    // Held without the app's reason, Next still says why.
    let next_reason = if blocked_reason.as_str().is_empty() {
        AzString::from_const_str(NEXT_HELD_REASON)
    } else {
        blocked_reason
    };
    buttons.push(button(
        forward_label,
        ButtonType::Primary,
        (!next_held).then_some(forward),
        next_reason,
    ));
    let buttons = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(BUTTONS_CLASS))
        .with_css_props(part(WIZARD_LAYOUT_BAND_BASE, &look.buttons))
        .with_children(DomVec::from_vec(buttons));

    let children: Vec<Dom> = if let Some(header) = header { alloc::vec![header, page_dom, buttons] } else {
        // The side panel: the glyph over the steps, done / current /
        // to come; the current one selected.
        let mut side: Vec<Dom> = Vec::with_capacity(count + 1);
        side.extend(glyph(&look.side_icon));
        for (i, label) in steps.as_ref().iter().enumerate() {
            let mark = match i.cmp(&current_step) {
                core::cmp::Ordering::Less => "\u{2713}",
                core::cmp::Ordering::Equal => "\u{25CF}",
                core::cmp::Ordering::Greater => "\u{25CB}",
            };
            let base = part(WIZARD_LAYOUT_SIDE_STEP_BASE, &look.side_step);
            let css = if i == current_step {
                stack_parts(
                    &base,
                    &CssPropertyWithConditionsVec::from_vec(look.side_step_current.clone()),
                )
            } else {
                base
            };
            side.push(
                Dom::create_div()
                    .with_ids_and_classes(classes_of(SIDE_STEP_CLASS))
                    .with_css_props(css)
                    .with_accessibility_info(AccessibilityInfo {
                        states: if i == current_step {
                            AccessibilityStateVec::from_vec(alloc::vec![
                                AccessibilityState::Selected
                            ])
                        } else {
                            AccessibilityStateVec::from_const_slice(&[])
                        },
                        ..AccessibilityInfo::named(label.clone(), AccessibilityRole::ListItem)
                    })
                    .with_children(DomVec::from_vec(alloc::vec![
                        text(AzString::from_const_str(mark), WIZARD_LAYOUT_FIXED_BASE, &[]),
                        text(label.clone(), WIZARD_LAYOUT_GROW_LABEL_BASE, &[]),
                    ])),
            );
        }
        let side_panel = Dom::create_div()
            .with_ids_and_classes(classes_of(SIDE_PANEL_CLASS))
            .with_css_props(part(WIZARD_LAYOUT_SIDE_PANEL_BASE, &look.side_panel))
            .with_accessibility_info(AccessibilityInfo::named(
                AzString::from_const_str("Steps"),
                AccessibilityRole::List,
            ))
            .with_children(DomVec::from_vec(side));
        let body = Dom::create_div()
            .with_ids_and_classes(classes_of(BODY_CLASS))
            .with_css_props(part(WIZARD_LAYOUT_BODY_BASE, &[]))
            .with_children(DomVec::from_vec(alloc::vec![side_panel, page_dom]));
        alloc::vec![body, buttons]
    };

    let mut classes: Vec<IdOrClass> = LAYOUT_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    // A standard size fixes the frame (the same in every theme).
    let mut base: Vec<CssPropertyWithConditions> = WIZARD_LAYOUT_BASE.to_vec();
    if size != WizardLayoutSize::Fill {
        base.push(CssPropertyWithConditions::simple(CssProperty::const_width(
            LayoutWidth::px(size.width()),
        )));
        base.push(CssPropertyWithConditions::simple(CssProperty::const_height(
            LayoutHeight::px(size.height()),
        )));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(&base, &look.layout))
        // A GROUP named "<title>: step i of n, <step>".
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Grouping,
            accessibility_name: Some(AzString::from(alloc::format!(
                "{}: step {} of {}, {}",
                title.as_str(),
                current_step + 1,
                count,
                step_label.as_str()
            )))
            .into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(children))
}

/// One class on a node.
fn classes_of(class: &'static str) -> IdOrClassVec {
    IdOrClassVec::from_vec(alloc::vec![Class(AzString::from_const_str(class))])
}

/// A part that keeps its size in its row (a glyph, a step's mark).
pub(crate) static WIZARD_LAYOUT_FIXED_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A column that takes the rest of its row (the banner's words).
pub(crate) static WIZARD_LAYOUT_GROW_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A label that takes the rest of its row (a step's name).
pub(crate) static WIZARD_LAYOUT_GROW_LABEL_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// The reason in the button row: the spacer's place, its text never
/// selected by a drag (the band's rule).
pub(crate) static WIZARD_LAYOUT_REASON_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// The row of the side panel beside the page: the rest of the layout.
pub(crate) static WIZARD_LAYOUT_BODY_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
];

/// The side panel: a column of fixed width, whose text a drag never selects.
pub(crate) static WIZARD_LAYOUT_SIDE_PANEL_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// One step of the side panel: its mark beside its name, on one midline.
pub(crate) static WIZARD_LAYOUT_SIDE_STEP_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
];

#[cfg(test)]
mod wizard_layout_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    };

    type Log = Arc<Mutex<Vec<(WizardEventKind, usize)>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: WizardEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push((event.kind, event.step));
        }
        Update::RefreshDom
    }

    fn strs(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    fn wizard(log: &Log, step: usize) -> WizardLayout {
        WizardLayout::create(
            AzString::from("Add account"),
            strs(&["Address", "Server", "Done"]),
        )
        .with_current_step(step)
        .with_page(Dom::create_p_with_text("Your e-mail address"))
        .with_on_event(RefAny::new(log.clone()), record as WizardOnEventCallbackType)
    }

    /// Every text of the subtree, in document order.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            out.push(s.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The node whose direct text child reads `label`.
    fn node_labelled(styled: &StyledDom, label: &str) -> NodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        for (i, nd) in styled.node_data.as_ref().iter().enumerate() {
            if let NodeType::Text(s) = nd.get_node_type() {
                if s.as_ref().as_str() == label {
                    return hierarchy[i].parent_id().expect("a label sits in its block");
                }
            }
        }
        panic!("no node is labelled {label:?}");
    }

    /// The button whose label reads `label`: the first keyboard stop above
    /// the label.
    fn button_labelled(styled: &StyledDom, label: &str) -> NodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let mut node = node_labelled(styled, label);
        loop {
            if nodes[node.index()].get_tab_index().is_some() {
                return node;
            }
            node = hierarchy[node.index()]
                .parent_id()
                .expect("a button label sits in its button");
        }
    }

    #[test]
    fn the_layout_is_the_rail_the_page_and_the_buttons() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = wizard(&log, 1).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 3, "{}: rail, page, buttons", theme.name());
            let mut rail = Vec::new();
            texts(&parts[0], &mut rail);
            for step in ["Address", "Server", "Done"] {
                assert!(rail.iter().any(|t| t == step), "{}: {step} on the rail", theme.name());
            }
            let mut page = Vec::new();
            texts(&parts[1], &mut page);
            assert_eq!(page, vec!["Add account", "Your e-mail address"], "{}", theme.name());
            let mut buttons = Vec::new();
            texts(&parts[2], &mut buttons);
            assert_eq!(buttons, vec!["Cancel", "Back", "Next"], "{}", theme.name());
        }
        let last = wizard(&log, 2).with_theme(UiTheme::Flat).dom();
        let mut buttons = Vec::new();
        texts(&last.children.as_ref()[2], &mut buttons);
        assert_eq!(buttons, vec!["Cancel", "Back", "Finish"], "Finish on the last step");
        let no_cancel = wizard(&log, 0)
            .with_labels(
                AzString::from("Zurueck"),
                AzString::from("Weiter"),
                AzString::from("Fertig stellen"),
                AzString::from(""),
            )
            .with_theme(UiTheme::Flat)
            .dom();
        let mut buttons = Vec::new();
        texts(&no_cancel.children.as_ref()[2], &mut buttons);
        assert_eq!(buttons, vec!["Zurueck", "Weiter"], "no Cancel without a label");
    }

    #[test]
    fn the_layout_is_a_group_that_says_which_step_it_is_on() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = wizard(&log, 1).with_theme(UiTheme::Flat).dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Grouping);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some("Add account: step 2 of 3, Server")
        );
        assert!(dom.root.get_tab_index().is_none());
    }

    /// The text a fired event showed as a tooltip (a held button's reason).
    fn shown_reason(fired: Option<(Update, Vec<crate::callbacks::CallbackChange>)>) -> Option<String> {
        fired?.1.into_iter().find_map(|c| match c {
            crate::callbacks::CallbackChange::ShowTooltip { text, .. } if !text.as_str().is_empty() => {
                Some(text.as_str().to_string())
            }
            _ => None,
        })
    }

    #[test]
    fn the_buttons_report_their_step_and_back_on_the_first_step_says_why_and_does_not_go_back() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(wizard(&log, 1).with_theme(UiTheme::Flat).dom());
        let click = |label: &str| {
            rv::fire(
                &styled,
                id(button_labelled(&styled, label)),
                EventFilter::Hover(HoverEventFilter::Click),
            )
        };
        let (update, _) = click("Next").expect("Next takes the click");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        click("Back").expect("Back takes the click on step 2");
        click("Cancel").expect("Cancel takes the click");
        assert_eq!(
            *log.lock().expect("log"),
            vec![
                (WizardEventKind::Next, 1),
                (WizardEventKind::Back, 1),
                (WizardEventKind::Cancel, 1)
            ]
        );

        let first = StyledDom::create_from_dom(wizard(&log, 0).with_theme(UiTheme::Flat).dom());
        let before = log.lock().expect("log").len();
        assert!(
            shown_reason(rv::fire(
                &first,
                id(button_labelled(&first, "Back")),
                EventFilter::Hover(HoverEventFilter::Click)
            ))
            .is_some(),
            "nothing to go back to: Back says so"
        );
        assert_eq!(log.lock().expect("log").len(), before, "and does not go back");
        let last = StyledDom::create_from_dom(wizard(&log, 2).with_theme(UiTheme::Flat).dom());
        rv::fire(
            &last,
            id(button_labelled(&last, "Finish")),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("Finish takes the click");
        assert_eq!(
            log.lock().expect("log").last(),
            Some(&(WizardEventKind::Finish, 2))
        );
        // Held without a reason of the app's: Next still says why.
        let held = StyledDom::create_from_dom(
            wizard(&log, 1)
                .with_can_go_next(false)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let before = log.lock().expect("log").len();
        assert!(
            shown_reason(rv::fire(
                &held,
                id(button_labelled(&held, "Next")),
                EventFilter::Hover(HoverEventFilter::MouseEnter)
            ))
            .is_some(),
            "a held Next says why while the page is not valid"
        );
        let _ = rv::fire(
            &held,
            id(button_labelled(&held, "Next")),
            EventFilter::Hover(HoverEventFilter::Click),
        );
        assert_eq!(log.lock().expect("log").len(), before, "and does not advance");
    }

    #[test]
    fn a_layout_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "wizard_layout",
            || wizard(&log, 1).dom(),
            |t: UiTheme| wizard(&log, 1).with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || wizard(&log, 1).dom());
            theme_checks::assert_structure_is_shared(
                &format!("wizard_layout built for {}", theme.name()),
                &dom,
                &[],
            );
        }
    }
    /// The declarations on `node` of type `ty`, as Debug strings.
    fn declared(node: &Dom, ty: azul_css::props::property::CssPropertyType) -> Vec<String> {
        node.root
            .style
            .iter_inline_properties()
            .filter(|(p, _)| p.get_type() == ty)
            .map(|(p, _)| format!("{p:?}"))
            .collect()
    }

    #[test]
    fn a_reason_holds_next_and_the_button_row_says_why() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let reason = "Accept the license agreement to continue.";
        for theme in checks::BOTH {
            let held = wizard(&log, 1)
                .with_validation(AzString::from(reason))
                .with_theme(theme);
            assert!(held.is_next_held());
            let dom = held.dom();
            let mut buttons = Vec::new();
            texts(&dom.children.as_ref()[2], &mut buttons);
            assert_eq!(
                buttons,
                vec!["Cancel", reason, "Back", "Next"],
                "{}: the reason sits before Back",
                theme.name()
            );
            let styled = StyledDom::create_from_dom(dom);
            let next = button_labelled_any(&styled, "Next");
            // User decision D1 (2026-10-05): a held Next shows why on hover
            // (and on keyboard focus: it keeps its Tab stop) and does not
            // advance.
            assert_eq!(
                styled.node_data.as_ref()[next.index()].get_tab_index(),
                Some(azul_core::dom::TabIndex::Auto),
                "{}: a held Next keeps its Tab stop",
                theme.name()
            );
            for event in [
                EventFilter::Hover(HoverEventFilter::MouseEnter),
                EventFilter::Focus(azul_core::events::FocusEventFilter::FocusReceived),
            ] {
                assert_eq!(
                    shown_reason(rv::fire(&styled, id(next), event)).as_deref(),
                    Some(reason),
                    "{}: a held Next shows why on {event:?}",
                    theme.name()
                );
            }
            let before = log.lock().expect("log").len();
            let _ = rv::fire(&styled, id(next), EventFilter::Hover(HoverEventFilter::Click));
            assert_eq!(
                log.lock().expect("log").len(),
                before,
                "{}: a held Next does not advance",
                theme.name()
            );
            let info = styled.node_data.as_ref()[next.index()]
                .get_accessibility_info()
                .expect("the Next button declares its role")
                .clone();
            assert!(
                info.states
                    .as_ref()
                    .contains(&azul_core::a11y::AccessibilityState::Unavailable),
                "{}: a held Next is announced unavailable",
                theme.name()
            );
            assert_eq!(
                info.description.as_ref().map(|d| d.as_str().to_string()),
                Some(reason.to_string()),
                "{}: a held Next is described by the reason",
                theme.name()
            );
        }
        let go = wizard(&log, 1).with_validation(AzString::from(""));
        assert!(!go.is_next_held(), "an empty reason lets Next go");
        assert!(go.can_go_next);
    }

    /// The node whose text reads `label`, walked up to the first node that
    /// declares the push button role.
    fn button_labelled_any(styled: &StyledDom, label: &str) -> NodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let mut node = node_labelled(styled, label);
        loop {
            if nodes[node.index()]
                .get_accessibility_info()
                .is_some_and(|i| i.role == azul_core::a11y::AccessibilityRole::PushButton)
            {
                return node;
            }
            node = hierarchy[node.index()]
                .parent_id()
                .expect("a button label sits in its button");
        }
    }

    #[test]
    fn back_held_by_the_app_says_why_and_does_not_go_back() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(
            wizard(&log, 1)
                .with_can_go_back(false)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let back = button_labelled_any(&styled, "Back");
        assert!(
            shown_reason(rv::fire(
                &styled,
                id(back),
                EventFilter::Hover(HoverEventFilter::MouseEnter)
            ))
            .is_some(),
            "Back held on an installer's progress page says why"
        );
        let _ = rv::fire(&styled, id(back), EventFilter::Hover(HoverEventFilter::Click));
        assert!(log.lock().expect("log").is_empty(), "and does not go back");
    }

    #[test]
    fn the_banner_shows_the_step_its_subtitle_and_the_glyph_and_no_rail() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = wizard(&log, 1)
                .with_style(WizardLayoutStyle::Banner)
                .with_subtitle(AzString::from("Where should AzOffice be installed?"))
                .with_icon(AzString::from("install_desktop"))
                .with_theme(theme)
                .dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 3, "{}: banner, page, buttons", theme.name());
            let mut banner = Vec::new();
            texts(&parts[0], &mut banner);
            // The glyph's text leaf is empty until the icon resolves.
            banner.retain(|t| !t.is_empty());
            assert_eq!(
                banner,
                vec!["Server", "Where should AzOffice be installed?"],
                "{}: the banner names the step, then the subtitle",
                theme.name()
            );
            assert!(
                theme_checks::nodes(&parts[0])
                    .iter()
                    .any(|(_, n)| matches!(n.root.get_node_type(), NodeType::Icon(_))),
                "{}: the banner carries the glyph",
                theme.name()
            );
            let mut page = Vec::new();
            texts(&parts[1], &mut page);
            assert_eq!(
                page,
                vec!["Your e-mail address"],
                "{}: the banner replaces the title line",
                theme.name()
            );
        }
    }

    #[test]
    fn the_side_panel_lists_the_steps_down_the_left_and_marks_the_current_one() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = wizard(&log, 1)
                .with_style(WizardLayoutStyle::SidePanel)
                .with_theme(theme)
                .dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 2, "{}: the body row, the buttons", theme.name());
            let body = parts[0].children.as_ref();
            assert_eq!(body.len(), 2, "{}: the side panel beside the page", theme.name());
            let steps = theme_checks::find_all(&body[0], SIDE_STEP_CLASS);
            assert_eq!(steps.len(), 3, "{}: one row per step", theme.name());
            let mut labels = Vec::new();
            for s in &steps {
                texts(s, &mut labels);
            }
            assert!(
                labels.iter().any(|t| t == "Server"),
                "{}: the steps are listed: {labels:?}",
                theme.name()
            );
            let current: Vec<bool> = steps
                .iter()
                .map(|s| {
                    s.root.get_accessibility_info().is_some_and(|i| {
                        i.states
                            .as_ref()
                            .contains(&azul_core::a11y::AccessibilityState::Selected)
                    })
                })
                .collect();
            assert_eq!(current, vec![false, true, false], "{}", theme.name());
            let mut page = Vec::new();
            texts(&body[1], &mut page);
            assert_eq!(page, vec!["Add account", "Your e-mail address"], "{}", theme.name());
        }
    }

    #[test]
    fn a_standard_size_fixes_the_frame_and_fill_takes_the_host() {
        use azul_css::props::property::CssPropertyType;
        assert_eq!(
            (WizardLayoutSize::Classic.width(), WizardLayoutSize::Classic.height()),
            (640.0, 480.0)
        );
        assert_eq!(WizardLayoutSize::Compact.width(), 500.0);
        assert_eq!(WizardLayoutSize::MacInstaller.height(), 440.0);
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let classic = wizard(&log, 0)
            .with_size(WizardLayoutSize::Classic)
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(
            declared(&classic, CssPropertyType::Width)
                .iter()
                .any(|d| d.contains("640")),
            "the classic frame is 640 wide: {:?}",
            declared(&classic, CssPropertyType::Width)
        );
        assert!(declared(&classic, CssPropertyType::Height)
            .iter()
            .any(|d| d.contains("480")));
        let fill = wizard(&log, 0).with_theme(UiTheme::Flat).dom();
        assert!(declared(&fill, CssPropertyType::Width).is_empty(), "Fill sets no width");
    }

    #[test]
    fn the_banner_and_the_side_panel_follow_the_app_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for style in [WizardLayoutStyle::Banner, WizardLayoutStyle::SidePanel] {
            checks::assert_follows_the_app_theme(
                "wizard_layout (frame)",
                || {
                    wizard(&log, 1)
                        .with_style(style)
                        .with_icon(AzString::from("install_desktop"))
                        .with_validation(AzString::from("Choose a folder."))
                        .dom()
                },
                |t: UiTheme| {
                    wizard(&log, 1)
                        .with_style(style)
                        .with_icon(AzString::from("install_desktop"))
                        .with_validation(AzString::from("Choose a folder."))
                        .with_theme(t)
                        .dom()
                },
            );
        }
    }
}
