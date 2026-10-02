//! ShellNavigationPane - Outlook 2010's navigation pane: collapsible GROUPS,
//! each a tree (the accounts and their folders, Favorites), a MODULE
//! SWITCHER of big buttons at the bottom (Mail, Calendar, Contacts, Tasks)
//! and a "collapse to strip" state that leaves a narrow bar of the module
//! icons (Outlook's minimized navigation pane).
//!
//! ```text
//! ┌──────────────────┐   ┌──┐
//! │ [ + New ]        │   │▸ │  the strip: an expand chevron and the
//! │ ▾ Favorites      │   │✉ │  module icons, each still a tab
//! │     Inbox    12  │   │📅│
//! │ ▾ me@example.org │   │👥│
//! │     Inbox    12  │   └──┘
//! │     Drafts    1  │
//! │   ▸ Archive      │
//! ├──────────────────┤
//! │ ✉ Mail       12  │  the module switcher: PageTabs, the active one
//! │ 📅 Calendar      │  selected; Up / Down / Home / End move between
//! │ 👥 Contacts      │  them, Enter or a click selects
//! │              [◂] │
//! └──────────────────┘
//! ```
//!
//! The groups are the [`Accordion`] in its Groups variant (Explorer's group
//! headers), each body a [`TreeView`]. The pane owns none of the state: it
//! reports every action through ONE callback ([`ShellNavigationPaneOnEvent`],
//! [`ShellNavigationPaneEvent`]) and the app rebuilds - which group is open,
//! which node is expanded or selected (`TreeViewNode::with_expanded` /
//! `with_selected`), which module is active, whether the pane is collapsed.
//!
//! The pane is a `<nav>` landmark named by its label. Its API is general -
//! groups + tree + module switcher - so AzMail (MAILWIDGETS) and AzDrive
//! build on it alike.
//!
//! Key types: [`ShellNavigationPane`], [`ShellNavigationGroup`], [`ShellNavigationModule`],
//! [`ShellNavigationPaneEvent`], [`ShellNavigationPaneOnEvent`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, NodeType, OptionDom},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    corety::OptionUsize,
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    AzString,
};

use super::{
    inner_theme, look_for, part, root_classes, stack_state, text, ShellLook, CHROME_ROW_BASE,
    GROW_LABEL_BASE, ITEM_BASE, LABEL_BASE, RAIL_BASE, ROW_BASE, SCROLL_COLUMN_BASE,
};
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        accordion::{
            Accordion, AccordionOnToggleCallbackType, AccordionSection, AccordionSectionVec,
            AccordionVariant,
        },
        badge::Badge,
        button::{Button, ButtonOnClickCallbackType},
        roving,
        themes::{OptionUiTheme, UiTheme},
        tree_view::{
            TreeView, TreeViewNode, TreeViewOnNodeClickCallbackType, TreeViewOnNodeDropCallbackType,
            TreeViewOnNodeToggleCallbackType,
        },
    },
};

/// The pane's class.
pub const NAV_CLASS: &str = "__azul-native-navigation-pane";
/// Added to the pane while it is collapsed to a strip.
pub const COLLAPSED_CLASS: &str = "__azul-native-navigation-pane-collapsed";
/// The header slot's class.
pub const HEADER_CLASS: &str = "__azul-native-navigation-pane-header";
/// The groups column's class.
pub const GROUPS_CLASS: &str = "__azul-native-navigation-pane-groups";
/// The module switcher's class.
pub const MODULES_CLASS: &str = "__azul-native-navigation-pane-modules";
/// A module button's class (in the switcher and in the strip).
pub const MODULE_CLASS: &str = "__azul-native-navigation-pane-module";
/// Added to the active module button.
pub const MODULE_ACTIVE_CLASS: &str = "__azul-native-navigation-pane-module-active";
/// A module button's icon.
pub const MODULE_ICON_CLASS: &str = "__azul-native-navigation-pane-module-icon";
/// A module button's label.
pub const MODULE_LABEL_CLASS: &str = "__azul-native-navigation-pane-module-label";
/// A module button's badge.
pub const MODULE_BADGE_CLASS: &str = "__azul-native-navigation-pane-module-badge";
/// The footer row's class (the collapse chevron).
pub const FOOTER_CLASS: &str = "__azul-native-navigation-pane-footer";
/// The strip's class (the collapsed pane's column of icons).
pub const STRIP_CLASS: &str = "__azul-native-navigation-pane-strip";
/// Added to a module button of the strip.
pub const STRIP_ITEM_CLASS: &str = "__azul-native-navigation-pane-strip-item";

/// What happened on the pane.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShellNavigationPaneEventKind {
    /// Group `group` was opened (`expand`) or closed.
    GroupToggled,
    /// Node `index` (the tree's depth-first index) of group `group` was
    /// clicked: the app selects it.
    NodeClicked,
    /// Node `index` of group `group` was expanded (`expand`) or collapsed.
    NodeToggled,
    /// Module `index` was chosen.
    ModuleSelected,
    /// The collapse chevron: `expand` asks for the expanded pane.
    CollapseToggled,
    /// A drag was dropped on node `index` of group `group` (the app moves
    /// what it dragged there: a task to a list, a message to a folder).
    NodeDropped,
}

/// One action on the pane: what, in which group, which node or module.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShellNavigationPaneEvent {
    /// The group (`GroupToggled`, `NodeClicked`, `NodeToggled`); 0 otherwise.
    pub group: usize,
    /// The node's depth-first index in its group's tree, or the module's
    /// index; 0 otherwise.
    pub index: usize,
    /// What happened.
    pub kind: ShellNavigationPaneEventKind,
    /// The new open / expanded state (`GroupToggled`, `NodeToggled`,
    /// `CollapseToggled`); false otherwise.
    pub expand: bool,
}

/// Callback invoked for every action on the pane.
pub type ShellNavigationPaneOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ShellNavigationPaneEvent) -> Update;
impl_widget_callback!(
    ShellNavigationPaneOnEvent,
    OptionShellNavigationPaneOnEvent,
    ShellNavigationPaneOnEventCallback,
    ShellNavigationPaneOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellNavigationPaneOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_NAVIGATION_PANE_ON_EVENT_INVOKER,
    invoker_ty:     AzShellNavigationPaneOnEventCallbackInvoker,
    thunk_fn:       az_shell_navigation_pane_on_event_callback_thunk,
    setter_fn:      AzApp_setShellNavigationPaneOnEventCallbackInvoker,
    from_handle_fn: AzShellNavigationPaneOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellNavigationPaneOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ShellNavigationPaneEvent ],
}

/// One collapsible group: a title, a count, a tree.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellNavigationGroup {
    /// The tree the group shows (its root is the first row).
    pub tree: TreeViewNode,
    /// The group's title ("Favorites", "me@example.org").
    pub title: AzString,
    /// A count after the title ("Folders (3)"), or none.
    pub count: OptionUsize,
    /// Whether the group is open.
    pub is_open: bool,
}

impl_option!(
    ShellNavigationGroup,
    OptionShellNavigationGroup,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ShellNavigationGroup,
    ShellNavigationGroupVec,
    ShellNavigationGroupVecDestructor,
    ShellNavigationGroupVecDestructorType,
    ShellNavigationGroupVecSlice,
    OptionShellNavigationGroup
);
impl_vec_clone!(ShellNavigationGroup, ShellNavigationGroupVec, ShellNavigationGroupVecDestructor);
impl_vec_debug!(ShellNavigationGroup, ShellNavigationGroupVec);
impl_vec_partialeq!(ShellNavigationGroup, ShellNavigationGroupVec);
impl_vec_mut!(ShellNavigationGroup, ShellNavigationGroupVec);

impl ShellNavigationGroup {
    /// An open group `title` showing `tree`.
    #[must_use]
    pub fn create(title: AzString, tree: TreeViewNode) -> Self {
        Self {
            tree,
            title,
            count: OptionUsize::None,
            is_open: true,
        }
    }

    /// A count after the title.
    pub const fn set_count(&mut self, count: usize) {
        self.count = OptionUsize::Some(count);
    }

    /// [`Self::set_count`] for the builder chain.
    #[must_use]
    pub const fn with_count(mut self, count: usize) -> Self {
        self.set_count(count);
        self
    }

    /// Open or close the group.
    pub const fn set_open(&mut self, open: bool) {
        self.is_open = open;
    }

    /// [`Self::set_open`] for the builder chain.
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }
}

/// One module of the switcher: a big button of icon and label, with an
/// optional badge (the unread count).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellNavigationModule {
    /// The label ("Mail").
    pub label: AzString,
    /// The icon (a `Dom::create_icon` name: "mail").
    pub icon: AzString,
    /// The badge text ("12"), or empty for none.
    pub badge: AzString,
}

impl_option!(
    ShellNavigationModule,
    OptionShellNavigationModule,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ShellNavigationModule,
    ShellNavigationModuleVec,
    ShellNavigationModuleVecDestructor,
    ShellNavigationModuleVecDestructorType,
    ShellNavigationModuleVecSlice,
    OptionShellNavigationModule
);
impl_vec_clone!(ShellNavigationModule, ShellNavigationModuleVec, ShellNavigationModuleVecDestructor);
impl_vec_debug!(ShellNavigationModule, ShellNavigationModuleVec);
impl_vec_partialeq!(ShellNavigationModule, ShellNavigationModuleVec);
impl_vec_mut!(ShellNavigationModule, ShellNavigationModuleVec);

impl ShellNavigationModule {
    /// A module with `label` and `icon`, no badge.
    #[must_use]
    pub fn create(label: AzString, icon: AzString) -> Self {
        Self {
            label,
            icon,
            badge: AzString::from_const_str(""),
        }
    }

    /// The badge text (empty: none).
    pub fn set_badge(&mut self, badge: AzString) {
        self.badge = badge;
    }

    /// [`Self::set_badge`] for the builder chain.
    #[must_use]
    pub fn with_badge(mut self, badge: AzString) -> Self {
        self.set_badge(badge);
        self
    }
}

/// Outlook's navigation pane: groups of trees, a module switcher, a
/// collapse-to-strip state.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ShellNavigationPane {
    /// The slot above the groups (a "+ New" button), or none.
    pub header: OptionDom,
    /// The groups, top to bottom.
    pub groups: ShellNavigationGroupVec,
    /// The modules of the switcher, top to bottom.
    pub modules: ShellNavigationModuleVec,
    /// Every action on the pane.
    pub on_event: OptionShellNavigationPaneOnEvent,
    /// The pane's accessible name.
    pub label: AzString,
    /// The active module.
    pub active_module: usize,
    /// The widget theme this pane is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// Collapsed to the strip of module icons.
    pub collapsed: bool,
}

impl ShellNavigationPane {
    /// An empty, expanded pane named "Navigation".
    #[must_use]
    pub fn create() -> Self {
        Self {
            header: OptionDom::None,
            groups: ShellNavigationGroupVec::from_const_slice(&[]),
            modules: ShellNavigationModuleVec::from_const_slice(&[]),
            on_event: None.into(),
            label: AzString::from_const_str("Navigation"),
            active_module: 0,
            theme: OptionUiTheme::None,
            collapsed: false,
        }
    }

    /// The slot above the groups.
    pub fn set_header(&mut self, header: Dom) {
        self.header = OptionDom::Some(header);
    }

    /// [`Self::set_header`] for the builder chain.
    #[must_use]
    pub fn with_header(mut self, header: Dom) -> Self {
        self.set_header(header);
        self
    }

    /// Appends a group.
    pub fn add_group(&mut self, group: ShellNavigationGroup) {
        let mut v = self.groups.clone().into_library_owned_vec();
        v.push(group);
        self.groups = ShellNavigationGroupVec::from_vec(v);
    }

    /// [`Self::add_group`] for the builder chain.
    #[must_use]
    pub fn with_group(mut self, group: ShellNavigationGroup) -> Self {
        self.add_group(group);
        self
    }

    /// Replaces the groups.
    pub fn set_groups(&mut self, groups: ShellNavigationGroupVec) {
        self.groups = groups;
    }

    /// [`Self::set_groups`] for the builder chain.
    #[must_use]
    pub fn with_groups(mut self, groups: ShellNavigationGroupVec) -> Self {
        self.set_groups(groups);
        self
    }

    /// Appends a module to the switcher.
    pub fn add_module(&mut self, module: ShellNavigationModule) {
        let mut v = self.modules.clone().into_library_owned_vec();
        v.push(module);
        self.modules = ShellNavigationModuleVec::from_vec(v);
    }

    /// [`Self::add_module`] for the builder chain.
    #[must_use]
    pub fn with_module(mut self, module: ShellNavigationModule) -> Self {
        self.add_module(module);
        self
    }

    /// Replaces the modules.
    pub fn set_modules(&mut self, modules: ShellNavigationModuleVec) {
        self.modules = modules;
    }

    /// [`Self::set_modules`] for the builder chain.
    #[must_use]
    pub fn with_modules(mut self, modules: ShellNavigationModuleVec) -> Self {
        self.set_modules(modules);
        self
    }

    /// The active module.
    pub const fn set_active_module(&mut self, index: usize) {
        self.active_module = index;
    }

    /// [`Self::set_active_module`] for the builder chain.
    #[must_use]
    pub const fn with_active_module(mut self, index: usize) -> Self {
        self.set_active_module(index);
        self
    }

    /// The pane's accessible name.
    pub fn set_label(&mut self, label: AzString) {
        self.label = label;
    }

    /// [`Self::set_label`] for the builder chain.
    #[must_use]
    pub fn with_label(mut self, label: AzString) -> Self {
        self.set_label(label);
        self
    }

    /// Collapse the pane to the strip, or expand it.
    pub const fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    /// [`Self::set_collapsed`] for the builder chain.
    #[must_use]
    pub const fn with_collapsed(mut self, collapsed: bool) -> Self {
        self.set_collapsed(collapsed);
        self
    }

    /// Every action on the pane.
    pub fn set_on_event<C: Into<ShellNavigationPaneOnEventCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_event = Some(ShellNavigationPaneOnEvent {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ShellNavigationPaneOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme; unset, the pane follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty pane and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// The pane's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for ShellNavigationPane {
    fn default() -> Self {
        Self::create()
    }
}

impl From<ShellNavigationPane> for Dom {
    fn from(p: ShellNavigationPane) -> Self {
        p.dom()
    }
}

// ---------------------------------------------------------------------------
// The report: every part hands its action to the one callback
// ---------------------------------------------------------------------------

/// What a part of the pane carries: which group or module it is, the app's
/// callback, and (for the collapse chevron) the state it asks for.
struct PartRef {
    on_event: OptionShellNavigationPaneOnEvent,
    group: usize,
    index: usize,
    expand: bool,
}

fn emit(
    data: &mut RefAny,
    info: CallbackInfo,
    kind: ShellNavigationPaneEventKind,
    group: usize,
    index: usize,
    expand: bool,
) -> Update {
    let Some(part) = data.downcast_ref::<PartRef>() else {
        return Update::DoNothing;
    };
    match part.on_event.as_ref() {
        Some(ShellNavigationPaneOnEvent { callback, refany }) => callback.invoke(
            refany.clone(),
            info,
            ShellNavigationPaneEvent {
                group,
                index,
                kind,
                expand,
            },
        ),
        None => Update::DoNothing,
    }
}

/// The group index a part carries.
fn group_of(data: &mut RefAny) -> usize {
    data.downcast_ref::<PartRef>().map_or(0, |p| p.group)
}

/// What the groups accordion carries: the app's callback and every group's
/// open state as built, so a toggle can report the NEW state.
struct GroupsRef {
    on_event: OptionShellNavigationPaneOnEvent,
    open: Vec<bool>,
}

extern "C" fn on_group_toggle(mut data: RefAny, info: CallbackInfo, section: usize) -> Update {
    // The accordion flips its own open state and reports the section; the
    // pane reports the group and the state it is in now - the opposite of
    // what it was built with.
    let Some(groups) = data.downcast_ref::<GroupsRef>() else {
        return Update::DoNothing;
    };
    let was_open = groups.open.get(section).copied().unwrap_or(false);
    match groups.on_event.as_ref() {
        Some(ShellNavigationPaneOnEvent { callback, refany }) => callback.invoke(
            refany.clone(),
            info,
            ShellNavigationPaneEvent {
                group: section,
                index: 0,
                kind: ShellNavigationPaneEventKind::GroupToggled,
                expand: !was_open,
            },
        ),
        None => Update::DoNothing,
    }
}

extern "C" fn on_tree_click(mut data: RefAny, info: CallbackInfo, node: usize) -> Update {
    let group = group_of(&mut data);
    emit(
        &mut data,
        info,
        ShellNavigationPaneEventKind::NodeClicked,
        group,
        node,
        false,
    )
}

extern "C" fn on_tree_toggle(
    mut data: RefAny,
    info: CallbackInfo,
    node: usize,
    expand: bool,
) -> Update {
    let group = group_of(&mut data);
    emit(
        &mut data,
        info,
        ShellNavigationPaneEventKind::NodeToggled,
        group,
        node,
        expand,
    )
}

extern "C" fn on_tree_drop(mut data: RefAny, info: CallbackInfo, node: usize) -> Update {
    let group = group_of(&mut data);
    emit(
        &mut data,
        info,
        ShellNavigationPaneEventKind::NodeDropped,
        group,
        node,
        false,
    )
}

extern "C" fn on_module_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let index = data.downcast_ref::<PartRef>().map_or(0, |p| p.index);
    emit(
        &mut data,
        info,
        ShellNavigationPaneEventKind::ModuleSelected,
        0,
        index,
        false,
    )
}

extern "C" fn on_collapse_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let expand = data.downcast_ref::<PartRef>().map_or(false, |p| p.expand);
    emit(
        &mut data,
        info,
        ShellNavigationPaneEventKind::CollapseToggled,
        0,
        0,
        expand,
    )
}

/// Up / Down / Home / End move the keyboard stop between the module
/// buttons (the switcher is one Tab stop, APG "roving tabindex"); Enter and
/// Space are the engine's synthetic click, which selects.
extern "C" fn on_module_key(_data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let step = match key {
        VirtualKeyCode::Up => roving::Step::Previous,
        VirtualKeyCode::Down => roving::Step::Next,
        VirtualKeyCode::Home => roving::Step::First,
        VirtualKeyCode::End => roving::Step::Last,
        _ => return Update::DoNothing,
    };
    let me = info.get_hit_node();
    let Some(parent) = info.get_parent(me) else {
        return Update::DoNothing;
    };
    let items = roving::items_of(&info, parent, MODULE_CLASS);
    let Some(current) = items.iter().position(|n| *n == me) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, items.len(), step, false) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    roving::move_stop(&mut info, &items, target);
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn hook(event: EventFilter, cb: usize, refany: RefAny) -> CoreCallbackData {
    CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany,
    }
}

/// One module button: in the switcher (icon, label, badge) or in the strip
/// (icon only, named by its label).
#[allow(clippy::too_many_arguments)]
fn module_node(
    module: ShellNavigationModule,
    index: usize,
    stop: usize,
    active: bool,
    in_strip: bool,
    on_event: &OptionShellNavigationPaneOnEvent,
    inner: Option<UiTheme>,
    look: &ShellLook,
) -> Dom {
    let ShellNavigationModule { label, icon, badge } = module;
    let mut classes: Vec<IdOrClass> = alloc::vec![IdOrClass::Class(AzString::from_const_str(MODULE_CLASS))];
    if in_strip {
        classes.push(IdOrClass::Class(AzString::from_const_str(STRIP_ITEM_CLASS)));
    }
    if active {
        classes.push(IdOrClass::Class(AzString::from_const_str(MODULE_ACTIVE_CLASS)));
    }
    let base = part(
        ITEM_BASE,
        if in_strip {
            &look.nav_strip_item
        } else {
            &look.nav_module
        },
    );
    let css = if active {
        stack_state(&base, &look.nav_module_active)
    } else {
        base
    };
    let mut children: Vec<Dom> = Vec::with_capacity(3);
    let icon_node = if icon.as_str().is_empty() {
        Dom::create_div()
    } else {
        Dom::create_icon(icon)
    };
    children.push(
        icon_node
            .with_class(AzString::from_const_str(MODULE_ICON_CLASS))
            .with_css_props(part(LABEL_BASE, &look.nav_module_icon)),
    );
    if !in_strip {
        children.push(
            text(label.clone())
                .with_class(AzString::from_const_str(MODULE_LABEL_CLASS))
                .with_css_props(part(GROW_LABEL_BASE, &look.nav_module_label)),
        );
        if !badge.as_str().is_empty() {
            let mut b = Badge::create(badge);
            if let Some(t) = inner {
                b = b.with_theme(t);
            }
            children.push(
                Dom::create_div()
                    .with_class(AzString::from_const_str(MODULE_BADGE_CLASS))
                    .with_css_props(part(LABEL_BASE, &[]))
                    .with_child(b.dom()),
            );
        }
    }
    let part_ref = RefAny::new(PartRef {
        on_event: on_event.clone(),
        group: 0,
        index,
        expand: false,
    });
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(css)
        .with_tab_index(roving::item_tab_index(index, stop))
        .with_accessibility_info(AccessibilityInfo {
            states: if active {
                AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Selected])
            } else {
                AccessibilityStateVec::from_const_slice(&[])
            },
            ..AccessibilityInfo::named(label, AccessibilityRole::PageTab)
        })
        .with_callbacks(
            alloc::vec![
                hook(
                    EventFilter::Hover(HoverEventFilter::Click),
                    on_module_click as usize,
                    part_ref.clone(),
                ),
                hook(
                    EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    on_module_key as usize,
                    part_ref,
                ),
            ]
            .into(),
        )
        .with_children(DomVec::from_vec(children))
}

/// The module switcher (or the strip): a tab list of the modules.
#[allow(clippy::too_many_arguments)]
fn switcher(
    modules: ShellNavigationModuleVec,
    active: usize,
    in_strip: bool,
    on_event: &OptionShellNavigationPaneOnEvent,
    inner: Option<UiTheme>,
    look: &ShellLook,
) -> Dom {
    let modules = modules.into_library_owned_vec();
    let stop = roving::stop_index(Some(active), modules.len());
    let items: Vec<Dom> = modules
        .into_iter()
        .enumerate()
        .map(|(i, m)| module_node(m, i, stop, i == active, in_strip, on_event, inner, look))
        .collect();
    let (class, base, skin) = if in_strip {
        (STRIP_CLASS, SCROLL_COLUMN_BASE, &look.nav_strip)
    } else {
        (MODULES_CLASS, CHROME_ROW_BASE, &look.nav_modules)
    };
    Dom::create_div()
        .with_class(AzString::from_const_str(class))
        .with_css_props(part(base, skin))
        .with_accessibility_info(AccessibilityInfo::named(
            "Modules",
            AccessibilityRole::PageTabList,
        ))
        .with_children(DomVec::from_vec(items))
}

/// The collapse / expand chevron in its footer row.
fn footer(collapsed: bool, on_event: &OptionShellNavigationPaneOnEvent, inner: Option<UiTheme>, look: &ShellLook) -> Dom {
    let (icon, name) = if collapsed {
        ("chevron_right", "Expand the navigation pane")
    } else {
        ("chevron_left", "Collapse the navigation pane")
    };
    let mut button = Button::create(AzString::from_const_str(""))
        .with_icon(AzString::from_const_str(icon))
        .with_on_click(
            RefAny::new(PartRef {
                on_event: on_event.clone(),
                group: 0,
                index: 0,
                expand: collapsed,
            }),
            on_collapse_click as ButtonOnClickCallbackType,
        );
    if let Some(t) = inner {
        button = button.with_theme(t);
    }
    Dom::create_div()
        .with_class(AzString::from_const_str(FOOTER_CLASS))
        .with_css_props(part(ROW_BASE, &look.nav_footer))
        .with_child(button.dom().with_accessibility_name(name))
}

/// The groups: the accordion in its Groups variant, a tree per body.
fn groups(
    groups: ShellNavigationGroupVec,
    on_event: &OptionShellNavigationPaneOnEvent,
    inner: Option<UiTheme>,
    look: &ShellLook,
) -> Dom {
    let groups = groups.into_library_owned_vec();
    let open: Vec<bool> = groups.iter().map(|g| g.is_open).collect();
    let sections: Vec<AccordionSection> = groups
        .into_iter()
        .enumerate()
        .map(|(i, g)| {
            let group_ref = RefAny::new(PartRef {
                on_event: on_event.clone(),
                group: i,
                index: 0,
                expand: g.is_open,
            });
            let mut tree = TreeView::new(g.tree)
                .with_on_node_click(group_ref.clone(), on_tree_click as TreeViewOnNodeClickCallbackType)
                .with_on_node_toggle(group_ref.clone(), on_tree_toggle as TreeViewOnNodeToggleCallbackType);
            // A pane the app listens to is a drop target: a drop on a node is
            // its `NodeDropped`.
            if on_event.is_some() {
                tree = tree.with_on_node_drop(group_ref, on_tree_drop as TreeViewOnNodeDropCallbackType);
            }
            if let Some(t) = inner {
                tree = tree.with_theme(t);
            }
            let mut section = AccordionSection::new(g.title, tree.dom()).with_open(g.is_open);
            if let OptionUsize::Some(n) = g.count {
                section = section.with_count(n);
            }
            section
        })
        .collect();
    // The accordion reports the section index; the groups ref knows the
    // open states, so the pane reports the state the group is in now.
    let mut accordion = Accordion::new(AccordionSectionVec::from_vec(sections))
        .with_variant(AccordionVariant::Groups)
        .with_on_toggle(
            RefAny::new(GroupsRef {
                on_event: on_event.clone(),
                open,
            }),
            on_group_toggle as AccordionOnToggleCallbackType,
        );
    if let Some(t) = inner {
        accordion = accordion.with_theme(t);
    }
    Dom::create_div()
        .with_class(AzString::from_const_str(GROUPS_CLASS))
        .with_css_props(part(SCROLL_COLUMN_BASE, &look.nav_groups))
        .with_child(accordion.dom())
}

/// The pane's DOM in `look`. Expanded: nav [header?, groups, modules,
/// footer]; collapsed: nav.collapsed [footer, strip].
pub(crate) fn build(pane: ShellNavigationPane, look: &ShellLook) -> Dom {
    let ShellNavigationPane {
        header,
        groups: pane_groups,
        modules,
        on_event,
        label,
        active_module,
        theme,
        collapsed,
    } = pane;
    let inner = inner_theme(theme);
    let mut classes = root_classes(NAV_CLASS, look).into_library_owned_vec();
    let mut children: Vec<Dom> = Vec::with_capacity(4);
    let skin = if collapsed {
        classes.push(IdOrClass::Class(AzString::from_const_str(COLLAPSED_CLASS)));
        children.push(footer(true, &on_event, inner, look));
        children.push(switcher(modules, active_module, true, &on_event, inner, look));
        &look.nav_strip
    } else {
        if let Some(h) = header.into_option() {
            children.push(
                Dom::create_div()
                    .with_class(AzString::from_const_str(HEADER_CLASS))
                    .with_css_props(part(CHROME_ROW_BASE, &look.nav_header))
                    .with_child(h),
            );
        }
        children.push(groups(pane_groups, &on_event, inner, look));
        if !modules.as_ref().is_empty() {
            children.push(switcher(modules, active_module, false, &on_event, inner, look));
        }
        children.push(footer(false, &on_event, inner, look));
        &look.nav_root
    };
    Dom::create_node(NodeType::Nav)
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(RAIL_BASE, skin))
        .with_accessibility_name(label)
        .with_children(DomVec::from_vec(children))
}

#[cfg(test)]
mod navigation_pane_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId},
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        shells::fixtures::navigation_pane,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    /// The pre-order indices of the nodes carrying `class` - the node ids a
    /// `StyledDom` of the same tree gives them.
    fn indices_of(dom: &Dom, class: &str) -> Vec<usize> {
        tc::nodes(dom)
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| tc::has_class(n, class))
            .map(|(i, _)| i)
            .collect()
    }

    fn node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    #[test]
    fn an_expanded_pane_is_a_nav_of_groups_a_switcher_and_a_footer() {
        let dom = navigation_pane().with_theme(UiTheme::Flat).dom();
        assert!(matches!(dom.root.get_node_type(), NodeType::Nav));
        assert_eq!(
            dom.root
                .get_accessibility_info()
                .and_then(|i| i.accessibility_name.as_ref().map(|s| s.as_str().to_string())),
            Some("Navigation".to_string())
        );
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 3, "groups, modules, footer");
        assert!(tc::has_class(&kids[0], GROUPS_CLASS));
        assert!(tc::has_class(&kids[1], MODULES_CLASS));
        assert!(tc::has_class(&kids[2], FOOTER_CLASS));
        // Two groups, each a section of the groups accordion with a tree.
        assert_eq!(tc::find_all(&dom, "__azul-native-accordion-section").len(), 2);
        assert_eq!(tc::find_all(&dom, "__azul-native-tree-view").len(), 2);
        // Three modules, the first active and the one Tab stop.
        let modules = tc::find_all(&dom, MODULE_CLASS);
        assert_eq!(modules.len(), 3);
        assert!(tc::has_class(modules[0], MODULE_ACTIVE_CLASS));
        assert!(!tc::has_class(modules[1], MODULE_ACTIVE_CLASS));
        assert_eq!(
            modules[0].root.get_accessibility_info().map(|i| i.role),
            Some(AccessibilityRole::PageTab)
        );
        assert_eq!(
            modules.iter().map(|m| m.root.get_tab_index()).collect::<Vec<_>>(),
            vec![
                Some(azul_core::dom::TabIndex::Auto),
                Some(azul_core::dom::TabIndex::NoKeyboardFocus),
                Some(azul_core::dom::TabIndex::NoKeyboardFocus)
            ]
        );
        // The second module shows its badge.
        assert!(tc::find(modules[1], MODULE_BADGE_CLASS).is_some());
        assert!(tc::find(modules[0], MODULE_BADGE_CLASS).is_none());
    }

    #[test]
    fn a_header_slot_comes_first() {
        let dom = navigation_pane()
            .with_header(Dom::create_div())
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(tc::has_class(&dom.children.as_ref()[0], HEADER_CLASS));
    }

    #[test]
    fn a_collapsed_pane_is_a_strip_of_the_module_icons_each_named() {
        let dom = navigation_pane()
            .with_collapsed(true)
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(tc::has_class(&dom, COLLAPSED_CLASS));
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 2, "footer, strip");
        assert!(tc::has_class(&kids[0], FOOTER_CLASS));
        assert!(tc::has_class(&kids[1], STRIP_CLASS));
        assert!(tc::find(&dom, GROUPS_CLASS).is_none());
        let items = tc::find_all(&dom, STRIP_ITEM_CLASS);
        assert_eq!(items.len(), 3);
        for item in &items {
            assert_eq!(item.children.as_ref().len(), 1, "the icon only");
        }
        assert_eq!(
            items[1]
                .root
                .get_accessibility_info()
                .and_then(|i| i.accessibility_name.as_ref().map(|s| s.as_str().to_string())),
            Some("Calendar".to_string())
        );
    }

    type Log = Arc<Mutex<Vec<ShellNavigationPaneEvent>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::DoNothing
    }

    fn logged() -> (Dom, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = navigation_pane()
            .with_on_event(RefAny::new(log.clone()), record as ShellNavigationPaneOnEventCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        (dom, log)
    }

    #[test]
    fn a_click_on_a_module_reports_it_and_the_chevron_asks_to_collapse() {
        let (dom, log) = logged();
        let styled = StyledDom::create_from_dom(dom.clone());
        let modules = indices_of(&dom, MODULE_CLASS);
        rv::fire(&styled, node(modules[2]), EventFilter::Hover(HoverEventFilter::Click))
            .expect("a module click");
        let chevron = indices_of(&dom, "__azul-native-button");
        rv::fire(&styled, node(*chevron.last().expect("chevron")), EventFilter::Hover(HoverEventFilter::Click))
            .expect("the chevron click");
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, ShellNavigationPaneEventKind::ModuleSelected);
        assert_eq!(events[0].index, 2);
        assert_eq!(events[1].kind, ShellNavigationPaneEventKind::CollapseToggled);
        assert!(!events[1].expand, "an expanded pane asks to collapse");
    }

    #[test]
    fn down_moves_the_stop_to_the_next_module_home_to_the_first_and_both_ends_wrap() {
        let (dom, _) = logged();
        let styled = StyledDom::create_from_dom(dom.clone());
        let modules = indices_of(&dom, MODULE_CLASS);
        let (_, changes) = rv::press(&styled, node(modules[0]), VirtualKeyCode::Down, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(modules[1])));
        assert!(rv::prevented(&changes));
        let (_, changes) = rv::press(&styled, node(modules[2]), VirtualKeyCode::Home, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(modules[0])));
        // WAI-ARIA APG tabs: the module buttons wrap at the ends.
        let (_, changes) = rv::press(&styled, node(modules[2]), VirtualKeyCode::Down, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(modules[0])), "Down on the last wraps to the first");
        let (_, changes) = rv::press(&styled, node(modules[0]), VirtualKeyCode::Up, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(modules[2])), "Up on the first wraps to the last");
        let (_, changes) =
            rv::press(&styled, node(modules[0]), VirtualKeyCode::Down, &[VirtualKeyCode::LControl]).expect("keys");
        assert!(rv::focus_request(&changes).is_none(), "Ctrl+Down is the app's");
    }

    #[test]
    fn a_pane_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "navigation_pane",
            || navigation_pane().dom(),
            |t: UiTheme| navigation_pane().with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "navigation_pane (collapsed)",
            || navigation_pane().with_collapsed(true).dom(),
            |t: UiTheme| navigation_pane().with_collapsed(true).with_theme(t).dom(),
        );
    }
}

/// A drop on a row of a group's tree is the pane's `NodeDropped`, with the
/// group and the tree's node (a task dropped on a list in AzTasks).
#[cfg(test)]
mod drop_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        callbacks::Update,
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, NodeType},
        refany::RefAny,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };
    use azul_css::AzString;

    use super::{
        ShellNavigationGroup, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind, ShellNavigationPaneOnEventCallbackType,
    };
    use crate::{
        callbacks::CallbackInfo,
        widgets::{roving::test_support as rv, themes::UiTheme, tree_view::TreeViewNode},
    };

    type Log = Arc<Mutex<Vec<ShellNavigationPaneEvent>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    /// The tree row whose label reads `label`.
    fn row(styled: &StyledDom, label: &str) -> DomNodeId {
        let data = styled.node_data.as_ref();
        let hierarchy = styled.node_hierarchy.as_ref();
        for (i, nd) in data.iter().enumerate() {
            let NodeType::Text(t) = nd.get_node_type() else {
                continue;
            };
            if t.as_ref().as_str() != label {
                continue;
            }
            let mut at = hierarchy[i].parent_id();
            while let Some(n) = at {
                let is_row = data[n.index()].get_ids_and_classes().as_ref().iter().any(
                    |c| matches!(c, IdOrClass::Class(s) if s.as_str() == "__azul-native-tree-view-row"),
                );
                if is_row {
                    return DomNodeId {
                        dom: DomId::ROOT_ID,
                        node: NodeHierarchyItemId::from_crate_internal(Some(n)),
                    };
                }
                at = hierarchy[n.index()].parent_id();
            }
        }
        panic!("no tree row reads {label:?}");
    }

    #[test]
    fn a_drop_on_a_tree_row_reports_its_group_and_node() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let lists = TreeViewNode::new("On this computer")
            .with_expanded(true)
            .with_child(TreeViewNode::new("Work"))
            .with_child(TreeViewNode::new("Home"));
        let dom = ShellNavigationPane::create()
            .with_group(ShellNavigationGroup::create(
                AzString::from_const_str("My Tasks"),
                TreeViewNode::new("All tasks"),
            ))
            .with_group(ShellNavigationGroup::create(AzString::from_const_str("My Lists"), lists))
            .with_on_event(RefAny::new(log.clone()), record as ShellNavigationPaneOnEventCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        let styled = StyledDom::create_from_dom(dom);
        let (update, _) = rv::fire(&styled, row(&styled, "Home"), EventFilter::Hover(HoverEventFilter::Drop))
            .expect("a tree row of the pane takes drops");
        assert_eq!(update, Update::RefreshDom);
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ShellNavigationPaneEventKind::NodeDropped);
        assert_eq!((events[0].group, events[0].index), (1, 2));
    }
}
