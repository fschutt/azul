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
//! Next the app forbade ([`WizardLayout::can_go_next`]) are inert. For
//! assistive technology the layout is a group named "<title>: step i of n,
//! <step>"; the buttons are named by their labels.
//!
//! Key types: [`WizardLayout`], [`WizardEvent`], [`WizardEventKind`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec, OptionDom},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option_inner,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutMinHeight, LayoutMinWidth,
        },
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{Button, ButtonOnClickCallbackType, ButtonType},
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
static BUTTON_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-wizard-layout-button",
))];

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
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Whether Next (or Finish) does anything: unset while the page is not
    /// valid yet, the button is inert.
    pub can_go_next: bool,
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
            theme: crate::widgets::themes::OptionUiTheme::None,
            can_go_next: true,
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
        // RED: the layout is not built yet.
        let _ = self;
        Dom::create_div()
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

/// The layout's DOM in `look`: layout [rail [stepper], page [title,
/// content?], buttons [cancel?, spacer, back, next | finish]]. Every part
/// is its base (the structure), then the look's skin; the rail and the
/// buttons are the toolkit's own widgets, pinned to the layout's theme (or
/// following the app theme with it).
pub(crate) fn build(wizard: WizardLayout, look: &WizardLayoutLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let last = wizard.is_last_step();
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
        theme,
        can_go_next,
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

    // The rail: the stepper, a click on a step asking the app.
    let mut rail = Stepper::create(steps)
        .with_current_step(current_step)
        .with_on_step_change(shared.clone(), on_rail_step as StepperOnStepChangeCallbackType);
    if let Some(theme) = theme {
        rail = rail.with_theme(theme);
    }
    let rail = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(RAIL_CLASS))
        .with_css_props(part(WIZARD_LAYOUT_BAND_BASE, &look.rail))
        .with_child(rail.dom());

    // The page: the title over the app's content.
    let mut page_dom = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(PAGE_CLASS))
        .with_css_props(part(WIZARD_LAYOUT_PAGE_BASE, &look.page))
        .with_child(
            crate::widgets::widget_p_with_text(title.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TITLE_CLASS))
                .with_css_props(part(&[], &look.title)),
        );
    if let Some(content) = page.into_option() {
        page_dom = page_dom.with_child(content);
    }

    // The buttons: a button in its box; an inert one has no click.
    let button = |label: AzString,
                  kind: ButtonType,
                  on_click: Option<ButtonOnClickCallbackType>| {
        let mut b = Button::with_type(label, kind);
        if let Some(on_click) = on_click {
            b = b.with_on_click(shared.clone(), on_click);
        }
        if let Some(theme) = theme {
            b = b.with_theme(theme);
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(BUTTON_CLASS))
            .with_css_props(part(WIZARD_LAYOUT_BUTTON_BASE, &look.button))
            .with_child(b.dom())
    };
    let mut buttons: Vec<Dom> = Vec::with_capacity(4);
    if !cancel_label.as_str().is_empty() {
        buttons.push(button(cancel_label, ButtonType::Default, Some(on_cancel)));
    }
    buttons.push(
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SPACER_CLASS))
            .with_css_props(part(WIZARD_LAYOUT_SPACER_BASE, &[])),
    );
    buttons.push(button(
        back_label,
        ButtonType::Default,
        (current_step > 0).then_some(on_back as ButtonOnClickCallbackType),
    ));
    if last {
        buttons.push(button(
            finish_label,
            ButtonType::Primary,
            can_go_next.then_some(on_finish as ButtonOnClickCallbackType),
        ));
    } else {
        buttons.push(button(
            next_label,
            ButtonType::Primary,
            can_go_next.then_some(on_next as ButtonOnClickCallbackType),
        ));
    }
    let buttons = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(BUTTONS_CLASS))
        .with_css_props(part(WIZARD_LAYOUT_BAND_BASE, &look.buttons))
        .with_children(DomVec::from_vec(buttons));

    let mut classes: Vec<IdOrClass> = LAYOUT_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(WIZARD_LAYOUT_BASE, &look.layout))
        // A GROUP named "<title>: step i of n, <step>".
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Grouping,
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
        .with_children(DomVec::from_vec(alloc::vec![rail, page_dom, buttons]))
}

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

    #[test]
    fn the_buttons_report_their_step_and_back_on_the_first_step_is_inert() {
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
        assert!(
            rv::fire(
                &first,
                id(button_labelled(&first, "Back")),
                EventFilter::Hover(HoverEventFilter::Click)
            )
            .is_none(),
            "nothing to go back to"
        );
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
        let held = StyledDom::create_from_dom(
            wizard(&log, 1)
                .with_can_go_next(false)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        assert!(
            rv::fire(
                &held,
                id(button_labelled(&held, "Next")),
                EventFilter::Hover(HoverEventFilter::Click)
            )
            .is_none(),
            "Next is inert while the page is not valid"
        );
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
}
