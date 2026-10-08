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
    impl_option,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutMinWidth,
        },
        property::CssProperty,
        basic::length::PercentageValue,
        style::{effects::StyleOpacity, StyleCursor, StyleUserSelect},
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
/// Added to the box of an arrow that cannot go: dimmed, announced unavailable.
pub const NAV_DISABLED_CLASS: &str = "__azul-native-address-bar-nav-disabled";

/// An arrow that cannot go is dimmed (the same in every theme).
pub(crate) static ADDRESS_BAR_NAV_DISABLED_BASE: &[CssPropertyWithConditions] =
    &[CssPropertyWithConditions::simple(CssProperty::const_opacity(
        StyleOpacity {
            inner: PercentageValue::const_new(40),
        },
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
    /// The "Recent locations" chevron right of Forward (`with_recent`): the
    /// app opens a menu of the places visited, at the hit node.
    Recent,
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
    /// Show Explorer's "Recent locations" chevron right of Forward, which
    /// reports [`AddressBarEventKind::Recent`].
    pub show_recent: bool,
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
            show_recent: false,
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

    /// Show (or hide) the "Recent locations" chevron right of Forward.
    pub const fn set_recent(&mut self, show_recent: bool) {
        self.show_recent = show_recent;
    }

    /// [`Self::set_recent`] for the builder chain.
    #[must_use]
    pub const fn with_recent(mut self, show_recent: bool) -> Self {
        self.set_recent(show_recent);
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

extern "C" fn on_recent(mut data: RefAny, info: CallbackInfo) -> Update {
    emit(
        &mut data,
        info,
        AddressBarEventKind::Recent,
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
        show_recent,
    } = bar;
    let theme = theme.into_option();
    let shared = RefAny::new(AddressBarShared { on_event });

    // An arrow or Refresh: an icon button in its box. One that cannot go
    // has no click, is dimmed and is announced unavailable.
    let nav = |icon: &'static str, enabled: bool, on_click: ButtonOnClickCallbackType| {
        let mut button = Button::create(AzString::from_const_str(""))
            .with_icon(AzString::from_const_str(icon));
        if enabled {
            button = button.with_on_click(shared.clone(), on_click);
        }
        if let Some(theme) = theme {
            button = button.with_theme(theme);
        }
        let mut button = button.dom();
        let mut classes: Vec<IdOrClass> = NAV_CLASS.to_vec();
        let mut style = part(ADDRESS_BAR_NAV_BASE, &look.nav);
        if !enabled {
            classes.push(Class(AzString::from_const_str(NAV_DISABLED_CLASS)));
            style = crate::widgets::themes::theme_blocks::stack_parts(
                &style,
                &CssPropertyWithConditionsVec::from_const_slice(ADDRESS_BAR_NAV_DISABLED_BASE),
            );
            let mut a11y = button
                .root
                .get_accessibility_info()
                .cloned()
                .unwrap_or_default();
            let mut states = a11y.states.clone().into_library_owned_vec();
            states.push(azul_core::a11y::AccessibilityState::Unavailable);
            a11y.states = azul_core::a11y::AccessibilityStateVec::from_vec(states);
            button.root.set_accessibility_info(a11y);
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(style)
            .with_child(button)
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
        .with_children(DomVec::from_vec({
            let mut parts = alloc::vec![
                nav("arrow_back", can_go_back, on_back),
                nav("arrow_forward", can_go_forward, on_forward),
            ];
            if show_recent {
                parts.push(nav("expand_more", true, on_recent));
            }
            parts.extend([
                nav("arrow_upward", can_go_up, on_up),
                field,
                nav("refresh", true, on_refresh),
                search_box,
            ]);
            parts
        }))
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

    /// Explorer's "Recent locations" chevron sits right of Forward: with
    /// `with_recent(true)` the bar grows that button, which reports `Recent`
    /// (the app opens its menu of places); without it the bar is unchanged.
    #[test]
    fn the_recent_locations_chevron_follows_forward_and_reports_recent() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let plain = bar(&log).with_theme(UiTheme::Flat).dom();
        assert_eq!(plain.children.as_ref().len(), 6, "no chevron unless asked");
        assert!(!bar(&log).show_recent);

        let styled = StyledDom::create_from_dom(
            bar(&log)
                .with_recent(true)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let parts = children(&styled, NodeId::new(0));
        assert_eq!(parts.len(), 7, "back, forward, recent, up, field, refresh, search");
        let chevron = children(&styled, parts[2])[0];
        assert!(click(&styled, chevron).is_some(), "the chevron takes the click");
        assert_eq!(
            events(&log),
            vec![(AddressBarEventKind::Recent, 0, String::new())]
        );
    }

    /// An arrow that cannot go is not only inert: it LOOKS unavailable
    /// (dimmed) and is announced so, like Explorer's greyed Back arrow.
    #[test]
    fn an_arrow_that_cannot_go_is_dimmed_and_announced_unavailable() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            // Back and Up can go, Forward cannot.
            let dom = bar(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            let dimmed = |node: &Dom| {
                node.root
                    .style
                    .iter_inline_properties()
                    .any(|(p, _)| match p {
                        CssProperty::Opacity(o) => {
                            o.get_property().map_or(false, |o| o.inner.normalized() < 0.75)
                        }
                        _ => false,
                    })
            };
            let unavailable = |node: &Dom| {
                node.children.as_ref()[0]
                    .root
                    .get_accessibility_info()
                    .map_or(false, |a| {
                        a.states
                            .as_ref()
                            .contains(&azul_core::a11y::AccessibilityState::Unavailable)
                    })
            };
            assert!(has_class(&parts[1], NAV_DISABLED_CLASS), "{}", theme.name());
            assert!(dimmed(&parts[1]), "{}: Forward is dimmed", theme.name());
            assert!(unavailable(&parts[1]), "{}: Forward is unavailable", theme.name());
            for i in [0, 2, 4] {
                assert!(!has_class(&parts[i], NAV_DISABLED_CLASS), "{}: part {i}", theme.name());
                assert!(!dimmed(&parts[i]), "{}: part {i} is not dimmed", theme.name());
                assert!(!unavailable(&parts[i]), "{}: part {i}", theme.name());
            }
        }
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

    /// The first node carrying `class`.
    fn node_with_class(styled: &StyledDom, class: &str) -> Option<NodeId> {
        styled
            .node_data
            .as_ref()
            .iter()
            .position(|nd| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .map(NodeId::new)
    }

    /// `root` and every node under it, in document order.
    fn subtree(styled: &StyledDom, root: NodeId) -> Vec<NodeId> {
        let mut out = vec![root];
        for child in children(styled, root) {
            out.extend(subtree(styled, child));
        }
        out
    }

    /// Whether `node` registered a handler for `event`.
    fn has_callback(styled: &StyledDom, node: NodeId, event: EventFilter) -> bool {
        styled.node_data.as_ref()[node.index()]
            .get_callbacks()
            .as_ref()
            .iter()
            .any(|cb| cb.event == event)
    }

    /// The node a click on `node` reaches first: `node` itself or its
    /// nearest ancestor with a click handler (the test runner fires one
    /// node's handler, it does not bubble).
    fn clickable_from(styled: &StyledDom, node: NodeId) -> Option<NodeId> {
        let click = EventFilter::Hover(HoverEventFilter::Click);
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut at = Some(node);
        while let Some(n) = at {
            if has_callback(styled, n, click) {
                return Some(n);
            }
            at = hierarchy[n.index()].parent_id();
        }
        None
    }

    /// The path field takes the keyboard as it opens. A click on the trail's
    /// empty space swaps the trail for the field; unless the field then
    /// holds the focus, neither Escape nor a click elsewhere (its focus
    /// loss) can ever end the edit, and the bar stays a text field until the
    /// next navigation - the "poor text field" AzDrive showed.
    #[test]
    fn the_path_field_takes_the_keyboard_when_it_opens() {
        use azul_core::dom::ComponentEventFilter;
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(
            bar(&log)
                .with_editing(true)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let mount = EventFilter::Component(ComponentEventFilter::AfterMount);
        let field = node_with_class(&styled, "__azul-native-address-bar-field")
            .expect("the bar has a path field");
        let mounted = subtree(&styled, field)
            .into_iter()
            .find(|n| has_callback(&styled, *n, mount))
            .expect("a node of the path field takes the keyboard when it mounts");
        let (_, changes) = rv::fire(&styled, id(mounted), mount).expect("the mount handler runs");
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, CallbackChange::SetFocusTarget { .. })),
            "the mount asks for the keyboard focus: {changes:?}"
        );
    }

    /// Every crumb goes where it names, the current folder's too: Explorer's
    /// last segment is a button like the others. A click on it used to fall
    /// through to the field under it and turn the trail into a text field.
    #[test]
    fn a_click_on_the_current_crumb_goes_there_instead_of_starting_an_edit() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let label = node_labelled(&styled, "Documents");
        let target = clickable_from(&styled, label).expect("something takes the click");
        click(&styled, target).expect("the click runs a handler");
        assert_eq!(
            events(&log),
            vec![(AddressBarEventKind::Crumb, 2, String::new())],
            "the current crumb reports itself, it does not start an edit"
        );
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
