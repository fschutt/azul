//! Module switcher widget - the bottom of a mail window's navigation pane:
//! the big buttons that switch the whole window between its modules (Mail,
//! Calendar, Contacts, Tasks), the active one pushed in, and the chevron
//! that collapses them to a strip of glyphs. Outlook 2010's module buttons.
//!
//! The switcher owns nothing: the app keeps the active module and whether
//! the strip is collapsed, hears a click (`on_select`) and the chevron
//! (`on_collapse`), and rebuilds. The buttons are the toolkit's [`Button`]
//! with the switcher's own look.
//!
//! KEYBOARD (WAI-ARIA APG tabs): the module buttons are ONE Tab stop - the
//! active module. Up / Down move between them and wrap; Enter / Space (a
//! click) switches. The chevron is its own stop.
//!
//! Key types: [`ModuleSwitcher`], [`SwitcherModule`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, Update},
    dom::{Dom, DomVec, EventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutJustifyContent, LayoutMinWidth,
        },
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        button::{Button, ButtonOnClickCallbackType},
        roving::{self, Step},
    },
};

static SWITCHER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-module-switcher"))];
static CHEVRON_ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-module-switcher-chevron-row",
))];
static CHEVRON_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-module-switcher-chevron",
))];
static MODULE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-module-switcher-module",
))];
/// The class string of [`MODULE_CLASS`], for the key handler.
const MODULE_CLASS_NAME: &str = "__azul-native-module-switcher-module";
static MODULE_ACTIVE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-module-switcher-module-active",
))];
static COLLAPSED_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-module-switcher-collapsed",
))];

/// Callback invoked when a module is clicked (the module's index).
pub type ModuleSwitcherOnSelectCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    ModuleSwitcherOnSelect,
    OptionModuleSwitcherOnSelect,
    ModuleSwitcherOnSelectCallback,
    ModuleSwitcherOnSelectCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ModuleSwitcherOnSelectCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MODULE_SWITCHER_ON_SELECT_INVOKER,
    invoker_ty:     AzModuleSwitcherOnSelectCallbackInvoker,
    thunk_fn:       az_module_switcher_on_select_callback_thunk,
    setter_fn:      AzApp_setModuleSwitcherOnSelectCallbackInvoker,
    from_handle_fn: AzModuleSwitcherOnSelectCallback_createFromHostHandle,
    from_handle_byref_fn: AzModuleSwitcherOnSelectCallback_createFromHostHandleByref,
    extra_args:     [ index: usize ],
}

/// Callback invoked when the chevron is clicked (whether the switcher
/// should now be collapsed).
pub type ModuleSwitcherOnCollapseCallbackType =
    extern "C" fn(RefAny, CallbackInfo, bool) -> Update;
impl_widget_callback!(
    ModuleSwitcherOnCollapse,
    OptionModuleSwitcherOnCollapse,
    ModuleSwitcherOnCollapseCallback,
    ModuleSwitcherOnCollapseCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ModuleSwitcherOnCollapseCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MODULE_SWITCHER_ON_COLLAPSE_INVOKER,
    invoker_ty:     AzModuleSwitcherOnCollapseCallbackInvoker,
    thunk_fn:       az_module_switcher_on_collapse_callback_thunk,
    setter_fn:      AzApp_setModuleSwitcherOnCollapseCallbackInvoker,
    from_handle_fn: AzModuleSwitcherOnCollapseCallback_createFromHostHandle,
    from_handle_byref_fn: AzModuleSwitcherOnCollapseCallback_createFromHostHandleByref,
    extra_args:     [ collapsed: bool ],
}

/// One module: its label and glyph.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherModule {
    /// The label ("Mail"); the button's name, collapsed too.
    pub label: AzString,
    /// The glyph (a `Dom::create_icon` name: "mail", "calendar_month",
    /// "contacts", "task").
    pub icon: AzString,
}

impl SwitcherModule {
    /// A module labelled `label` with the glyph `icon`.
    #[must_use]
    pub const fn create(label: AzString, icon: AzString) -> Self {
        Self { label, icon }
    }
}

impl_option!(
    SwitcherModule,
    OptionSwitcherModule,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    SwitcherModule,
    SwitcherModuleVec,
    SwitcherModuleVecDestructor,
    SwitcherModuleVecDestructorType,
    SwitcherModuleVecSlice,
    OptionSwitcherModule
);
impl_vec_clone!(SwitcherModule, SwitcherModuleVec, SwitcherModuleVecDestructor);
impl_vec_debug!(SwitcherModule, SwitcherModuleVec);
impl_vec_partialeq!(SwitcherModule, SwitcherModuleVec);
impl_vec_mut!(SwitcherModule, SwitcherModuleVec);

/// The module switcher: big buttons, or a strip of glyphs.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleSwitcher {
    /// The modules, top to bottom.
    pub modules: SwitcherModuleVec,
    /// A module was clicked.
    pub on_select: OptionModuleSwitcherOnSelect,
    /// The chevron was clicked: collapse (true) or expand.
    pub on_collapse: OptionModuleSwitcherOnCollapse,
    /// The active module.
    pub active: usize,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Collapsed to a strip of glyphs.
    pub collapsed: bool,
}

/// What a theme decides about a module switcher: the SKIN of each part,
/// laid over the part's base (the switcher's structure, the same in every
/// theme: `MODULE_SWITCHER_*_BASE`) by [`build`].
pub(crate) struct ModuleSwitcherLook {
    /// The switcher.
    pub switcher: Vec<CssPropertyWithConditions>,
    /// The chevron's row.
    pub chevron_row: Vec<CssPropertyWithConditions>,
    /// The chevron button's container (a [`Button`] part style).
    pub chevron: Vec<CssPropertyWithConditions>,
    /// A module button's container at rest (a [`Button`] part style).
    pub module: Vec<CssPropertyWithConditions>,
    /// Added to the active module's container.
    pub module_active: Vec<CssPropertyWithConditions>,
    /// A module button's label (a [`Button`] part style).
    pub module_label: Vec<CssPropertyWithConditions>,
    /// A module button's glyph (a [`Button`] part style).
    pub module_icon: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the switcher, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the switcher's structure, in every theme ----

/// The switcher: a column of its chevron row and its buttons.
pub(crate) static MODULE_SWITCHER_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The chevron's row: the chevron at its end.
pub(crate) static MODULE_SWITCHER_CHEVRON_ROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::End,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// A module button's container: its glyph and label on one midline, the
/// label set left (collapsed: the glyph centred).
pub(crate) static MODULE_SWITCHER_MODULE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Start,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

impl ModuleSwitcher {
    /// A switcher of `modules`, the first active, expanded.
    #[must_use]
    pub fn create(modules: SwitcherModuleVec) -> Self {
        Self {
            modules,
            on_select: None.into(),
            on_collapse: None.into(),
            active: 0,
            theme: crate::widgets::themes::OptionUiTheme::None,
            collapsed: false,
        }
    }

    /// The active module.
    pub const fn set_active(&mut self, active: usize) {
        self.active = active;
    }

    /// [`Self::set_active`] for the builder chain.
    #[must_use]
    pub const fn with_active(mut self, active: usize) -> Self {
        self.set_active(active);
        self
    }

    /// Collapsed to a strip of glyphs.
    pub const fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    /// [`Self::set_collapsed`] for the builder chain.
    #[must_use]
    pub const fn with_collapsed(mut self, collapsed: bool) -> Self {
        self.set_collapsed(collapsed);
        self
    }

    /// Pin the widget theme; unset, the switcher follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// A module was clicked.
    pub fn set_on_select<C: Into<ModuleSwitcherOnSelectCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_select = Some(ModuleSwitcherOnSelect {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_select`] for the builder chain.
    #[must_use]
    pub fn with_on_select<C: Into<ModuleSwitcherOnSelectCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_select(data, cb);
        self
    }

    /// The chevron was clicked.
    pub fn set_on_collapse<C: Into<ModuleSwitcherOnCollapseCallback>>(
        &mut self,
        data: RefAny,
        cb: C,
    ) {
        self.on_collapse = Some(ModuleSwitcherOnCollapse {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_collapse`] for the builder chain.
    #[must_use]
    pub fn with_on_collapse<C: Into<ModuleSwitcherOnCollapseCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_collapse(data, cb);
        self
    }

    /// Replaces `self` with an empty switcher and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(SwitcherModuleVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    /// The switcher's DOM. The look comes from the theme module
    /// (`themes::flat::module_switcher` / `themes::flora::module_switcher`);
    /// `None` carries both looks, each in its `@theme(<name>)` block, and
    /// the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::module_switcher(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::module_switcher(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::module_switcher,
                crate::widgets::themes::flora::module_switcher,
            ),
        }
    }
}

impl Default for ModuleSwitcher {
    fn default() -> Self {
        Self::create(SwitcherModuleVec::from_const_slice(&[]))
    }
}

impl From<ModuleSwitcher> for Dom {
    fn from(s: ModuleSwitcher) -> Self {
        s.dom()
    }
}

/// What every part of one switcher shares: the app's hooks and the state.
struct SwitcherShared {
    on_select: OptionModuleSwitcherOnSelect,
    on_collapse: OptionModuleSwitcherOnCollapse,
    collapsed: bool,
}

/// A module button's payload.
struct ModuleData {
    index: usize,
    shared: RefAny,
}

extern "C" fn on_module_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let (index, mut shared) = {
        let Some(m) = data.downcast_ref::<ModuleData>() else {
            return Update::DoNothing;
        };
        (m.index, m.shared.clone())
    };
    let Some(shared) = shared.downcast_ref::<SwitcherShared>() else {
        return Update::DoNothing;
    };
    match shared.on_select.as_ref() {
        Some(ModuleSwitcherOnSelect { callback, refany }) => {
            callback.invoke(refany.clone(), info, index)
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_chevron_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(shared) = data.downcast_ref::<SwitcherShared>() else {
        return Update::DoNothing;
    };
    let collapse = !shared.collapsed;
    match shared.on_collapse.as_ref() {
        Some(ModuleSwitcherOnCollapse { callback, refany }) => {
            callback.invoke(refany.clone(), info, collapse)
        }
        None => Update::DoNothing,
    }
}

/// Up / Down on a module button (WAI-ARIA APG tabs): move the stop to the
/// neighbouring module, wrapping at the ends; Home / End to the first /
/// last. Moving is not switching: Enter or Space (a click) switches.
extern "C" fn on_module_key(_data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    let step = match roving::plain_key(&info.get_current_keyboard_state()) {
        Some(K::Up) => Step::Previous,
        Some(K::Down) => Step::Next,
        Some(K::Home) => Step::First,
        Some(K::End) => Step::Last,
        _ => return Update::DoNothing,
    };
    let focused = info.get_hit_node();
    let Some(switcher) = info.get_parent(focused) else {
        return Update::DoNothing;
    };
    let modules = roving::items_of(&info, switcher, MODULE_CLASS_NAME);
    let Some(current) = modules.iter().position(|n| *n == focused) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, modules.len(), step, true) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    if target != current {
        roving::move_stop(&mut info, &modules, target);
    }
    Update::DoNothing
}

/// The switcher's DOM in `look`: switcher [chevron row [chevron], module..].
/// Every part is its base (the structure), then the look's skin; the
/// buttons are the toolkit's own [`Button`] with the look's part styles
/// injected (the status bar's composition rule), pinned to the switcher's
/// theme (or following the app theme with it).
pub(crate) fn build(switcher: ModuleSwitcher, look: &ModuleSwitcherLook) -> Dom {
    use azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec;

    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let ModuleSwitcher {
        modules,
        on_select,
        on_collapse,
        active,
        theme,
        collapsed,
    } = switcher;
    let theme = theme.into_option();
    let shared = RefAny::new(SwitcherShared {
        on_select,
        on_collapse,
        collapsed,
    });

    // The chevron: down to collapse the buttons to glyphs, up to expand.
    let mut chevron = Button::create(AzString::from_const_str(""))
        .with_icon(AzString::from_const_str(if collapsed {
            "keyboard_double_arrow_up"
        } else {
            "keyboard_double_arrow_down"
        }))
        .with_on_click(shared.clone(), on_chevron_click as ButtonOnClickCallbackType);
    chevron.alt = AzString::from_const_str(if collapsed {
        "Expand the navigation pane"
    } else {
        "Collapse the navigation pane"
    });
    chevron.container_style = OptionCssPropertyWithConditionsVec::Some(part(&[], &look.chevron));
    if let Some(theme) = theme {
        chevron = chevron.with_theme(theme);
    }
    let chevron_row = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CHEVRON_ROW_CLASS))
        .with_css_props(part(MODULE_SWITCHER_CHEVRON_ROW_BASE, &look.chevron_row))
        .with_child(
            chevron
                .dom()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CHEVRON_CLASS)),
        );

    let count = modules.as_ref().len();
    let stop = roving::stop_index(Some(active), count);
    let mut children: Vec<Dom> = Vec::with_capacity(count + 1);
    children.push(chevron_row);
    for (index, module) in modules.into_library_owned_vec().into_iter().enumerate() {
        let SwitcherModule { label, icon } = module;
        let is_active = index == active;
        let data = RefAny::new(ModuleData {
            index,
            shared: shared.clone(),
        });
        // Collapsed: the glyph alone; the label stays the name.
        let name = label.clone();
        let mut b = Button::create(if collapsed {
            AzString::from_const_str("")
        } else {
            label.clone()
        })
        .with_icon(icon)
        .with_on_click(data.clone(), on_module_click as ButtonOnClickCallbackType);
        b.alt = label;
        let mut container = look.module.clone();
        if is_active {
            container.extend(look.module_active.iter().cloned());
        }
        b.container_style =
            OptionCssPropertyWithConditionsVec::Some(part(MODULE_SWITCHER_MODULE_BASE, &container));
        b.label_style = OptionCssPropertyWithConditionsVec::Some(part(&[], &look.module_label));
        b.icon_style = OptionCssPropertyWithConditionsVec::Some(part(&[], &look.module_icon));
        if let Some(theme) = theme {
            b = b.with_theme(theme);
        }
        let mut classes: Vec<IdOrClass> = MODULE_CLASS.to_vec();
        if is_active {
            classes.push(MODULE_ACTIVE_CLASS[0].clone());
        }
        children.push(
            b.dom()
                .with_ids_and_classes(IdOrClassVec::from_vec(classes))
                .with_tab_index(roving::item_tab_index(index, stop))
                // A TAB of the window's modules, the active one selected.
                .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                    role: azul_core::a11y::AccessibilityRole::PageTab,
                    accessibility_name: Some(name).into(),
                    states: if is_active {
                        azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
                            azul_core::a11y::AccessibilityState::Selected,
                        ])
                    } else {
                        azul_core::a11y::AccessibilityStateVec::from_const_slice(&[])
                    },
                    ..Default::default()
                })
                .with_callback(
                    EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    data,
                    CoreCallback {
                        cb: on_module_key as usize,
                        ctx: OptionRefAny::None,
                    },
                ),
        );
    }

    let mut classes: Vec<IdOrClass> = SWITCHER_CLASS.to_vec();
    if collapsed {
        classes.push(COLLAPSED_CLASS[0].clone());
    }
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(MODULE_SWITCHER_BASE, &look.switcher))
        // The LIST of the window's module tabs.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::PageTabList,
            accessibility_name: Some(AzString::from_const_str("Modules")).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(children))
}

#[cfg(test)]
mod module_switcher_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, HoverEventFilter, NodeId, NodeType, TabIndex},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    };

    type Log = Arc<Mutex<Vec<String>>>;

    extern "C" fn record_select(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(format!("select {index}"));
        }
        Update::RefreshDom
    }

    extern "C" fn record_collapse(mut data: RefAny, _: CallbackInfo, collapsed: bool) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(format!("collapse {collapsed}"));
        }
        Update::RefreshDom
    }

    fn modules() -> SwitcherModuleVec {
        SwitcherModuleVec::from_vec(vec![
            SwitcherModule::create(AzString::from("Mail"), AzString::from("mail")),
            SwitcherModule::create(AzString::from("Calendar"), AzString::from("calendar_month")),
            SwitcherModule::create(AzString::from("Contacts"), AzString::from("contacts")),
            SwitcherModule::create(AzString::from("Tasks"), AzString::from("task")),
        ])
    }

    fn switcher(log: &Log) -> ModuleSwitcher {
        ModuleSwitcher::create(modules())
            .with_active(1)
            .with_on_select(
                RefAny::new(log.clone()),
                record_select as ModuleSwitcherOnSelectCallbackType,
            )
            .with_on_collapse(
                RefAny::new(log.clone()),
                record_collapse as ModuleSwitcherOnCollapseCallbackType,
            )
    }

    /// Every text of the subtree, in document order.
    /// The visible texts. An icon's empty text leaf (`Dom::create_icon`
    /// holds one for the resolved glyph) is not a text the user reads.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            if !s.as_ref().as_str().is_empty() {
                out.push(s.as_ref().as_str().to_string());
            }
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

    fn name_of(node: &Dom) -> String {
        node.root
            .get_accessibility_info()
            .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string()))
            .unwrap_or_default()
    }

    #[test]
    fn expanded_the_switcher_is_the_chevron_over_one_big_button_per_module() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = switcher(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 5, "{}: the chevron row and four modules", theme.name());
            assert!(theme_checks::find(&parts[0], "__azul-native-module-switcher-chevron").is_some());
            assert_eq!(
                name_of(theme_checks::find(&parts[0], "__azul-native-module-switcher-chevron").expect("chevron")),
                "Collapse the navigation pane"
            );
            let mut labels = Vec::new();
            for m in &parts[1..] {
                texts(m, &mut labels);
                assert!(theme_checks::has_class(m, "__azul-native-module-switcher-module"));
                assert_eq!(
                    m.root.get_accessibility_info().map(|i| i.role),
                    Some(azul_core::a11y::AccessibilityRole::PageTab)
                );
            }
            assert_eq!(labels, vec!["Mail", "Calendar", "Contacts", "Tasks"], "{}", theme.name());
            assert_eq!(name_of(&parts[2]), "Calendar");
            assert!(theme_checks::has_class(&parts[2], "__azul-native-module-switcher-module-active"));
            assert_eq!(
                parts[2].root.get_accessibility_info().map(|i| i.states.as_ref().to_vec()),
                Some(vec![azul_core::a11y::AccessibilityState::Selected])
            );
            assert!(parts[1].root.get_accessibility_info().map_or(true, |i| i.states.as_ref().is_empty()));
            assert_ne!(parts[1].root.get_style(), parts[2].root.get_style(), "the active module is pushed in");
            assert_eq!(
                dom.root.get_accessibility_info().map(|i| i.role),
                Some(azul_core::a11y::AccessibilityRole::PageTabList)
            );
        }
    }

    #[test]
    fn collapsed_the_switcher_is_a_strip_of_glyphs_that_keep_their_names() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = switcher(&log).with_collapsed(true).with_theme(UiTheme::Flat).dom();
        assert!(theme_checks::has_class(&dom, "__azul-native-module-switcher-collapsed"));
        let parts = dom.children.as_ref();
        let mut labels = Vec::new();
        for m in &parts[1..] {
            texts(m, &mut labels);
            assert!(
                theme_checks::nodes(m).iter().any(|(_, n)| matches!(n.root.get_node_type(), NodeType::Icon(_))),
                "the glyph stays"
            );
        }
        assert!(labels.iter().all(|l| l.is_empty()), "no label text collapsed: {labels:?}");
        assert_eq!(name_of(&parts[1]), "Mail", "the name is the label still");
        assert_eq!(
            name_of(theme_checks::find(&parts[0], "__azul-native-module-switcher-chevron").expect("chevron")),
            "Expand the navigation pane"
        );
    }

    #[test]
    fn a_module_click_selects_it_and_the_chevron_toggles_the_collapse() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(switcher(&log).with_theme(UiTheme::Flat).dom());
        let hierarchy = styled.node_hierarchy.as_ref();
        let chevron_row = hierarchy[0].first_child_id(NodeId::new(0)).expect("the chevron row");
        let chevron = hierarchy[chevron_row.index()].first_child_id(chevron_row).expect("the chevron");
        let mail = hierarchy[chevron_row.index()].next_sibling_id().expect("Mail");
        let calendar = hierarchy[mail.index()].next_sibling_id().expect("Calendar");
        let contacts = hierarchy[calendar.index()].next_sibling_id().expect("Contacts");
        let click = |n: NodeId| {
            rv::fire(&styled, id(n), EventFilter::Hover(HoverEventFilter::Click)).expect("a click target")
        };
        let (update, _) = click(contacts);
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        click(chevron);
        assert_eq!(*log.lock().expect("log"), vec!["select 2".to_string(), "collapse true".to_string()]);

        let collapsed = StyledDom::create_from_dom(
            switcher(&log).with_collapsed(true).with_theme(UiTheme::Flat).dom(),
        );
        let hierarchy = collapsed.node_hierarchy.as_ref();
        let chevron_row = hierarchy[0].first_child_id(NodeId::new(0)).expect("the chevron row");
        let chevron = hierarchy[chevron_row.index()].first_child_id(chevron_row).expect("the chevron");
        rv::fire(&collapsed, id(chevron), EventFilter::Hover(HoverEventFilter::Click)).expect("a click target");
        assert_eq!(log.lock().expect("log").last().map(String::as_str), Some("collapse false"));
    }

    #[test]
    fn the_active_module_is_the_one_tab_stop_and_the_arrows_wrap_around_the_modules() {
        use azul_core::window::VirtualKeyCode as K;

        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(switcher(&log).with_theme(UiTheme::Flat).dom());
        let hierarchy = styled.node_hierarchy.as_ref();
        let chevron_row = hierarchy[0].first_child_id(NodeId::new(0)).expect("the chevron row");
        let mut modules = vec![hierarchy[chevron_row.index()].next_sibling_id().expect("Mail")];
        while let Some(next) = hierarchy[modules[modules.len() - 1].index()].next_sibling_id() {
            modules.push(next);
        }
        assert_eq!(modules.len(), 4);
        let stops: Vec<bool> = modules
            .iter()
            .map(|m| styled.node_data.as_ref()[m.index()].get_tab_index() == Some(TabIndex::Auto))
            .collect();
        assert_eq!(stops, vec![false, true, false, false], "the active module holds the stop");

        let (_, changes) = rv::press(&styled, id(modules[1]), K::Down, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(modules[2])));
        assert!(rv::prevented(&changes));
        let (_, changes) = rv::press(&styled, id(modules[0]), K::Up, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(modules[3])), "Up on the first wraps to the last");
        let (_, changes) = rv::press(&styled, id(modules[3]), K::Down, &[]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), Some(id(modules[0])), "Down on the last wraps to the first");
        assert!(log.lock().expect("log").is_empty(), "moving the stop switches nothing");
    }

    #[test]
    fn a_switcher_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for collapsed in [false, true] {
            let what = if collapsed { "module_switcher (collapsed)" } else { "module_switcher" };
            checks::assert_follows_the_app_theme(
                what,
                || switcher(&log).with_collapsed(collapsed).dom(),
                |t: UiTheme| switcher(&log).with_collapsed(collapsed).with_theme(t).dom(),
            );
            for theme in checks::BOTH {
                let dom = checks::under(theme, || switcher(&log).with_collapsed(collapsed).dom());
                theme_checks::assert_structure_is_shared(
                    &format!("{what} built for {}", theme.name()),
                    &dom,
                    &[],
                );
            }
        }
    }
}
