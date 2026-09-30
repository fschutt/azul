//! Address bar widget - the top strip of a file manager: Back, Forward and
//! Up, the current path as a breadcrumb trail whose crumbs navigate and whose
//! chevrons open a menu of the folder's siblings, a click on the trail's
//! empty space that turns it into an editable path field, Refresh, and a
//! search box. Windows Explorer's address bar.
//!
//! The bar owns nothing: the app keeps the history, the path and the search
//! text, hears every action through ONE callback ([`AddressBar::on_event`],
//! an [`AddressBarEvent`] naming what happened) and rebuilds. The parts are
//! the toolkit's own widgets: [`Button`] for the arrows and Refresh,
//! [`Breadcrumb`] (with its segment menus) for the trail, [`TextInput`] for
//! the editable path and the search box.
//!
//! Key types: [`AddressBar`], [`AddressBarEvent`], [`AddressBarEventKind`],
//! [`AddressBarOnEvent`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
    },
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_option_inner,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutMinWidth,
        },
        property::CssProperty,
        style::{StyleCursor, StyleUserSelect},
    },
    AzString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        breadcrumb::{Breadcrumb, BreadcrumbOnNavigateCallbackType, BreadcrumbState},
        button::{Button, ButtonOnClickCallbackType},
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnFocusLostCallbackType,
            TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
            TextInputState, TextInputValid,
        },
    },
};

static BAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-address-bar"))];
static NAV_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-nav",
))];
static FIELD_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-field",
))];
static SEARCH_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-address-bar-search",
))];

/// What happened on the bar.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AddressBarEventKind {
    /// The Back arrow.
    Back,
    /// The Forward arrow.
    Forward,
    /// The Up arrow.
    Up,
    /// The Refresh button.
    Refresh,
    /// A crumb of the trail (`index`): go there.
    Crumb,
    /// The chevron after a crumb (`index`): the app opens a menu of that
    /// folder's entries, at the hit node (`CallbackInfo::open_menu_for_hit_node`).
    CrumbMenu,
    /// A click on the trail's empty space: the app rebuilds with
    /// `editing` set and the path in `path`.
    EditStarted,
    /// Enter in the path field: `text` is the path typed.
    PathEntered,
    /// Escape in the path field, or it lost focus: the app rebuilds with
    /// `editing` unset.
    EditCancelled,
    /// The search box changed: `text` is its text.
    Search,
}

/// One action on the bar: what, which crumb, what text.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressBarEvent {
    /// The path typed (`PathEntered`) or the search text (`Search`); empty
    /// otherwise.
    pub text: AzString,
    /// The crumb (`Crumb`, `CrumbMenu`); 0 otherwise.
    pub index: usize,
    /// What happened.
    pub kind: AddressBarEventKind,
}

/// Callback invoked for every action on the bar.
pub type AddressBarOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, AddressBarEvent) -> Update;
impl_widget_callback!(
    AddressBarOnEvent,
    OptionAddressBarOnEvent,
    AddressBarOnEventCallback,
    AddressBarOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        AddressBarOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ADDRESS_BAR_ON_EVENT_INVOKER,
    invoker_ty:     AzAddressBarOnEventCallbackInvoker,
    thunk_fn:       az_address_bar_on_event_callback_thunk,
    setter_fn:      AzApp_setAddressBarOnEventCallbackInvoker,
    from_handle_fn: AzAddressBarOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzAddressBarOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: AddressBarEvent ],
}

/// A file manager's address bar: navigation arrows, the path as a trail or
/// a field, Refresh and a search box.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct AddressBar {
    /// The trail, the drive first, the open folder last.
    pub crumbs: StringVec,
    /// The path shown in the field while `editing`.
    pub path: AzString,
    /// The search box's text.
    pub search: AzString,
    /// The search box's prompt when it is empty ("Search Documents").
    pub search_placeholder: AzString,
    /// Hears every action on the bar.
    pub on_event: OptionAddressBarOnEvent,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Whether Back does anything (a history behind); an arrow that cannot
    /// go reports nothing.
    pub can_go_back: bool,
    /// Whether Forward does anything.
    pub can_go_forward: bool,
    /// Whether Up does anything (not at a drive's root).
    pub can_go_up: bool,
    /// Show the path as an editable field instead of the trail.
    pub editing: bool,
}

/// What a theme decides about an address bar: the SKIN of each part, laid
/// over the part's base (the bar's structure, the same in every theme:
/// `ADDRESS_BAR_*_BASE`) by [`build`].
pub(crate) struct AddressBarLook {
    /// The bar.
    pub bar: Vec<CssPropertyWithConditions>,
    /// The box around each arrow and Refresh (its spacing).
    pub nav: Vec<CssPropertyWithConditions>,
    /// The path field holding the trail.
    pub field: Vec<CssPropertyWithConditions>,
    /// The path field holding the text input (which draws its own box).
    pub field_editing: Vec<CssPropertyWithConditions>,
    /// The box around the search input.
    pub search: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the bar, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the bar's structure, in every theme ----

/// The bar: one row, its parts centred on the midline.
pub(crate) static ADDRESS_BAR_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// An arrow's box keeps its size.
pub(crate) static ADDRESS_BAR_NAV_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The field takes the rest of the bar; its trail sits on the midline and
/// a drag across it never selects its text.
pub(crate) static ADDRESS_BAR_FIELD_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The search box keeps its width.
pub(crate) static ADDRESS_BAR_SEARCH_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

impl AddressBar {
    /// A bar showing `crumbs`, nothing to go back, forward or up to, no
    /// search text.
    #[must_use]
    pub fn create(crumbs: StringVec) -> Self {
        Self {
            crumbs,
            path: AzString::from_const_str(""),
            search: AzString::from_const_str(""),
            search_placeholder: AzString::from_const_str("Search"),
            on_event: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
            can_go_back: false,
            can_go_forward: false,
            can_go_up: false,
            editing: false,
        }
    }

    /// The path the field shows while editing.
    pub fn set_path(&mut self, path: AzString) {
        self.path = path;
    }

    /// [`Self::set_path`] for the builder chain.
    #[must_use]
    pub fn with_path(mut self, path: AzString) -> Self {
        self.set_path(path);
        self
    }

    /// The search box's text.
    pub fn set_search(&mut self, search: AzString) {
        self.search = search;
    }

    /// [`Self::set_search`] for the builder chain.
    #[must_use]
    pub fn with_search(mut self, search: AzString) -> Self {
        self.set_search(search);
        self
    }

    /// The search box's prompt.
    pub fn set_search_placeholder(&mut self, placeholder: AzString) {
        self.search_placeholder = placeholder;
    }

    /// [`Self::set_search_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_search_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_search_placeholder(placeholder);
        self
    }

    /// Whether Back, Forward and Up do anything.
    pub const fn set_can_go(&mut self, back: bool, forward: bool, up: bool) {
        self.can_go_back = back;
        self.can_go_forward = forward;
        self.can_go_up = up;
    }

    /// [`Self::set_can_go`] for the builder chain.
    #[must_use]
    pub const fn with_can_go(mut self, back: bool, forward: bool, up: bool) -> Self {
        self.set_can_go(back, forward, up);
        self
    }

    /// Show the path field instead of the trail.
    pub const fn set_editing(&mut self, editing: bool) {
        self.editing = editing;
    }

    /// [`Self::set_editing`] for the builder chain.
    #[must_use]
    pub const fn with_editing(mut self, editing: bool) -> Self {
        self.set_editing(editing);
        self
    }

    /// Pin the widget theme; unset, the bar follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The callback that hears every action on the bar.
    pub fn set_on_event<C: Into<AddressBarOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = Some(AddressBarOnEvent {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<AddressBarOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Replaces `self` with an empty bar and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    /// The bar's DOM. The look comes from the theme module
    /// (`themes::flat::address_bar` / `themes::flora::address_bar`); `None`
    /// carries both looks, each in its `@theme(<name>)` block, and the app
    /// theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::address_bar(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::address_bar(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::address_bar,
                crate::widgets::themes::flora::address_bar,
            ),
        }
    }
}

impl Default for AddressBar {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

impl From<AddressBar> for Dom {
    fn from(b: AddressBar) -> Self {
        b.dom()
    }
}

/// What every part of one bar shares: the app's callback.
struct AddressBarShared {
    on_event: OptionAddressBarOnEvent,
}

/// Hands `kind` to the app's callback.
fn emit(
    data: &mut RefAny,
    info: CallbackInfo,
    kind: AddressBarEventKind,
    index: usize,
    text: AzString,
) -> Update {
    let Some(shared) = data.downcast_ref::<AddressBarShared>() else {
        return Update::DoNothing;
    };
    match shared.on_event.as_ref() {
        Some(AddressBarOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, AddressBarEvent { text, index, kind })
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_back(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(
        &mut data,
        info,
        AddressBarEventKind::Back,
        0,
        AzString::from_const_str(""),
    )
}

extern "C" fn on_forward(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(
        &mut data,
        info,
        AddressBarEventKind::Forward,
        0,
        AzString::from_const_str(""),
    )
}

extern "C" fn on_up(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(
        &mut data,
        info,
        AddressBarEventKind::Up,
        0,
        AzString::from_const_str(""),
    )
}

extern "C" fn on_refresh(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(
        &mut data,
        info,
        AddressBarEventKind::Refresh,
        0,
        AzString::from_const_str(""),
    )
}

/// A click on the field's empty space: the trail turns into a path field.
extern "C" fn on_field_click(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(
        &mut data,
        info,
        AddressBarEventKind::EditStarted,
        0,
        AzString::from_const_str(""),
    )
}

/// A crumb was clicked: go there. The click stops at the crumb, so the
/// field under it does not start an edit.
extern "C" fn on_crumb(mut data: RefAny, mut info: CallbackInfo, state: BreadcrumbState) -> Update {
    info.stop_propagation();
    emit(
        &mut data,
        info,
        AddressBarEventKind::Crumb,
        state.selected_index,
        AzString::from_const_str(""),
    )
}

/// A crumb's chevron was clicked: the app opens that folder's menu.
extern "C" fn on_crumb_menu(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: BreadcrumbState,
) -> Update {
    info.stop_propagation();
    emit(
        &mut data,
        info,
        AddressBarEventKind::CrumbMenu,
        state.selected_index,
        AzString::from_const_str(""),
    )
}

/// What a key in the path field asks for: Enter takes the path, Escape
/// gives the edit up, every other key is the field's own.
#[must_use]
pub(crate) fn path_key_event(key: Option<VirtualKeyCode>) -> Option<AddressBarEventKind> {
    match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            Some(AddressBarEventKind::PathEntered)
        }
        Some(VirtualKeyCode::Escape) => Some(AddressBarEventKind::EditCancelled),
        _ => None,
    }
}

extern "C" fn on_path_key(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match path_key_event(key) {
        Some(kind) => emit(&mut data, info, kind, 0, AzString::from(state.get_text())),
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The path field lost focus: the edit is given up.
extern "C" fn on_path_focus_lost(
    mut data: RefAny,
    info: CallbackInfo,
    _state: TextInputState,
) -> Update {
    emit(
        &mut data,
        info,
        AddressBarEventKind::EditCancelled,
        0,
        AzString::from_const_str(""),
    )
}

/// The search box changed.
extern "C" fn on_search_text(
    mut data: RefAny,
    info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let update = emit(
        &mut data,
        info,
        AddressBarEventKind::Search,
        0,
        AzString::from(state.get_text()),
    );
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The bar's DOM in `look`: [Back, Forward, Up, field (the trail, or the
/// path input while editing), Refresh, search]. Every part is its base (the
/// structure), then the look's skin; the buttons and inputs are the
/// toolkit's own widgets, pinned to the bar's theme (or following the app
/// theme with it).
pub(crate) fn build(bar: AddressBar, look: &AddressBarLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let AddressBar {
        crumbs,
        path,
        search,
        search_placeholder,
        on_event,
        theme,
        can_go_back,
        can_go_forward,
        can_go_up,
        editing,
    } = bar;
    let theme = theme.into_option();
    let shared = RefAny::new(AddressBarShared { on_event });

    // An arrow or Refresh: an icon button in its box. One that cannot go
    // has no click.
    let nav = |icon: &'static str, enabled: bool, on_click: ButtonOnClickCallbackType| {
        let mut button = Button::create(AzString::from_const_str(""))
            .with_icon(AzString::from_const_str(icon));
        if enabled {
            button = button.with_on_click(shared.clone(), on_click);
        }
        if let Some(theme) = theme {
            button = button.with_theme(theme);
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(NAV_CLASS))
            .with_css_props(part(ADDRESS_BAR_NAV_BASE, &look.nav))
            .with_child(button.dom())
    };

    let field = if editing {
        let mut input = TextInput::create()
            .with_text(path)
            .with_on_virtual_key_down(
                shared.clone(),
                on_path_key as TextInputOnVirtualKeyDownCallbackType,
            )
            .with_on_focus_lost(
                shared.clone(),
                on_path_focus_lost as TextInputOnFocusLostCallbackType,
            );
        if let Some(theme) = theme {
            input = input.with_theme(theme);
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(FIELD_CLASS))
            .with_css_props(part(ADDRESS_BAR_FIELD_BASE, &look.field_editing))
            .with_child(input.dom())
    } else {
        let mut trail = Breadcrumb::create(crumbs)
            .with_on_navigate(
                shared.clone(),
                on_crumb as BreadcrumbOnNavigateCallbackType,
            )
            .with_on_segment_menu(
                shared.clone(),
                on_crumb_menu as BreadcrumbOnNavigateCallbackType,
            );
        if let Some(theme) = theme {
            trail = trail.with_theme(theme);
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(FIELD_CLASS))
            .with_css_props(part(ADDRESS_BAR_FIELD_BASE, &look.field))
            .with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: on_field_click as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: shared.clone(),
                }]
                .into(),
            )
            .with_child(trail.dom())
    };

    let mut search_input = TextInput::create_search()
        .with_text(search)
        .with_placeholder(search_placeholder)
        .with_on_text_input(
            shared.clone(),
            on_search_text as TextInputOnTextInputCallbackType,
        );
    if let Some(theme) = theme {
        search_input = search_input.with_theme(theme);
    }
    let search_box = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SEARCH_CLASS))
        .with_css_props(part(ADDRESS_BAR_SEARCH_BASE, &look.search))
        .with_child(search_input.dom());

    let mut classes: Vec<IdOrClass> = BAR_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(ADDRESS_BAR_BASE, &look.bar))
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Toolbar,
            ..Default::default()
        })
        .with_children(DomVec::from_vec(alloc::vec![
            nav("arrow_back", can_go_back, on_back),
            nav("arrow_forward", can_go_forward, on_forward),
            nav("arrow_upward", can_go_up, on_up),
            field,
            nav("refresh", true, on_refresh),
            search_box,
        ]))
}

#[cfg(test)]
mod address_bar_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::{
        callbacks::CallbackChange,
        widgets::{
            roving::test_support as rv,
            themes::{theme_blocks::checks, UiTheme},
        },
    };

    type Log = Arc<Mutex<Vec<(AddressBarEventKind, usize, String)>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: AddressBarEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push((
                event.kind,
                event.index,
                event.text.as_str().to_string(),
            ));
        }
        Update::RefreshDom
    }

    fn events(log: &Log) -> Vec<(AddressBarEventKind, usize, String)> {
        log.lock().expect("log").clone()
    }

    fn labels(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    fn bar(log: &Log) -> AddressBar {
        AddressBar::create(labels(&["This PC", "Home", "Documents"]))
            .with_path(AzString::from("/Users/me/Documents"))
            .with_search_placeholder(AzString::from("Search Documents"))
            .with_can_go(true, false, true)
            .with_on_event(RefAny::new(log.clone()), record as AddressBarOnEventCallbackType)
    }

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// Whether any node of the subtree carries `class`.
    fn subtree_has_class(node: &Dom, class: &str) -> bool {
        has_class(node, class) || node.children.as_ref().iter().any(|c| subtree_has_class(c, class))
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

    fn children(styled: &StyledDom, parent: NodeId) -> Vec<NodeId> {
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut out = Vec::new();
        let mut cur = hierarchy[parent.index()].first_child_id(parent);
        while let Some(n) = cur {
            out.push(n);
            cur = hierarchy[n.index()].next_sibling_id();
        }
        out
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

    fn click(
        styled: &StyledDom,
        node: NodeId,
    ) -> Option<(Update, Vec<CallbackChange>)> {
        rv::fire(styled, id(node), EventFilter::Hover(HoverEventFilter::Click))
    }

    #[test]
    fn the_bar_is_back_forward_up_the_field_refresh_and_search() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = bar(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(parts.len(), 6, "{}", theme.name());
            for i in [0, 1, 2, 4] {
                assert!(
                    has_class(&parts[i], "__azul-native-address-bar-nav"),
                    "{}: part {i} is a navigation button",
                    theme.name()
                );
            }
            assert!(has_class(&parts[3], "__azul-native-address-bar-field"));
            assert!(
                subtree_has_class(&parts[3], "__azul-native-breadcrumb"),
                "{}: the field holds the trail",
                theme.name()
            );
            assert!(has_class(&parts[5], "__azul-native-address-bar-search"));
            assert_eq!(parts[5].children.as_ref().len(), 1, "the search input");
            assert_eq!(
                dom.root.get_accessibility_info().map(|i| i.role),
                Some(azul_core::a11y::AccessibilityRole::Toolbar)
            );
        }
    }

    #[test]
    fn the_crumbs_are_the_trail_and_the_search_box_carries_its_prompt() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = bar(&log).with_theme(UiTheme::Flat).dom();
        let mut found = Vec::new();
        texts(&dom.children.as_ref()[3], &mut found);
        for label in ["This PC", "Home", "Documents"] {
            assert!(found.iter().any(|t| t == label), "{label} in {found:?}");
        }
        fn prompted(node: &Dom, prompt: &str) -> bool {
            node.root.get_placeholder() == Some(prompt)
                || node.children.as_ref().iter().any(|c| prompted(c, prompt))
        }
        assert!(
            prompted(&dom.children.as_ref()[5], "Search Documents"),
            "the search input carries its prompt"
        );
    }

    #[test]
    fn a_click_on_the_field_starts_editing_and_the_editing_bar_shows_a_path_input() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let field = children(&styled, NodeId::new(0))[3];
        let (update, _) = click(&styled, field).expect("the field takes the click");
        assert_eq!(events(&log), vec![(AddressBarEventKind::EditStarted, 0, String::new())]);
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");

        let editing = bar(&log)
            .with_editing(true)
            .with_theme(UiTheme::Flat)
            .dom();
        let field = &editing.children.as_ref()[3];
        assert!(
            !subtree_has_class(field, "__azul-native-breadcrumb"),
            "no trail while editing"
        );
        assert_eq!(field.children.as_ref().len(), 1, "the path input");
        assert!(
            field.root.get_callbacks().as_ref().is_empty(),
            "the input takes the clicks now"
        );
    }

    #[test]
    fn back_up_and_refresh_report_and_an_arrow_that_cannot_go_is_inert() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let parts = children(&styled, NodeId::new(0));
        let button = |i: usize| children(&styled, parts[i])[0];
        assert!(click(&styled, button(0)).is_some(), "Back");
        assert!(click(&styled, button(1)).is_none(), "Forward cannot go: no click");
        assert!(click(&styled, button(2)).is_some(), "Up");
        assert!(click(&styled, button(4)).is_some(), "Refresh");
        assert_eq!(
            events(&log)
                .into_iter()
                .map(|(k, _, _)| k)
                .collect::<Vec<_>>(),
            vec![
                AddressBarEventKind::Back,
                AddressBarEventKind::Up,
                AddressBarEventKind::Refresh
            ]
        );
    }

    #[test]
    fn a_crumb_click_goes_there_and_its_chevron_opens_the_menu_without_starting_an_edit() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let home = node_labelled(&styled, "Home");
        let (_, changes) = click(&styled, home).expect("a crumb takes the click");
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, CallbackChange::StopPropagation)),
            "the click stops at the crumb"
        );
        // The chevron after "Home" is the crumb's next sibling.
        let chevron = styled.node_hierarchy.as_ref()[home.index()]
            .next_sibling_id()
            .expect("a chevron follows the crumb");
        let (_, changes) = click(&styled, chevron).expect("a chevron takes the click");
        assert!(changes
            .iter()
            .any(|c| matches!(c, CallbackChange::StopPropagation)));
        assert_eq!(
            events(&log),
            vec![
                (AddressBarEventKind::Crumb, 1, String::new()),
                (AddressBarEventKind::CrumbMenu, 1, String::new()),
            ]
        );
    }

    #[test]
    fn enter_in_the_path_field_takes_the_path_and_escape_gives_the_edit_up() {
        assert_eq!(
            path_key_event(Some(VirtualKeyCode::Return)),
            Some(AddressBarEventKind::PathEntered)
        );
        assert_eq!(
            path_key_event(Some(VirtualKeyCode::NumpadEnter)),
            Some(AddressBarEventKind::PathEntered)
        );
        assert_eq!(
            path_key_event(Some(VirtualKeyCode::Escape)),
            Some(AddressBarEventKind::EditCancelled)
        );
        assert_eq!(path_key_event(Some(VirtualKeyCode::A)), None);
        assert_eq!(path_key_event(None), None);
    }

    #[test]
    fn an_address_bar_without_a_theme_follows_the_app_theme() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "address_bar",
            || bar(&log).dom(),
            |t: UiTheme| bar(&log).with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "address_bar (editing)",
            || bar(&log).with_editing(true).dom(),
            |t: UiTheme| bar(&log).with_editing(true).with_theme(t).dom(),
        );
    }
}
