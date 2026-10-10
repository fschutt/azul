//! Tree view widget with expandable/collapsible nodes.
//!
//! Provides [`TreeView`] and [`TreeViewNode`] for building hierarchical
//! tree structures with click callbacks and recursive DOM rendering.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelector, PseudoStateType,
        ThemeCondition,
    },
    props::{
        basic::{
            color::{ColorOrSystem, ColorU},
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::CssProperty,
        style::*,
    },
    *,
};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
};

use crate::callbacks::{Callback, CallbackInfo};

// -- Callback type via macro --

/// Callback invoked when a tree node is clicked.
///
/// The `usize` parameter is the depth-first index of the clicked node
/// (0 = root, then incremented in pre-order traversal).
pub type TreeViewOnNodeClickCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    TreeViewOnNodeClick,
    OptionTreeViewOnNodeClick,
    TreeViewOnNodeClickCallback,
    TreeViewOnNodeClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TreeViewOnNodeClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TREE_VIEW_ON_NODE_CLICK_INVOKER,
    invoker_ty:     AzTreeViewOnNodeClickCallbackInvoker,
    thunk_fn:       az_tree_view_on_node_click_callback_thunk,
    setter_fn:      AzApp_setTreeViewOnNodeClickCallbackInvoker,
    from_handle_fn: AzTreeViewOnNodeClickCallback_createFromHostHandle,
    from_handle_byref_fn: AzTreeViewOnNodeClickCallback_createFromHostHandleByref,
    extra_args:     [ node_index: usize ],
}

/// Callback invoked when the KEYBOARD asks to open or close a node (WAI-ARIA
/// APG tree view: Right on a closed parent, Left on an open one).
///
/// `node_index` is the node's depth-first index, exactly as for
/// [`TreeViewOnNodeClickCallbackType`]; `expand` is the state asked for -
/// `true` to open, `false` to close. The tree does not own expansion: the app
/// stores it and rebuilds with [`TreeViewNode::with_expanded`]. A click is
/// still reported through `on_node_click` only.
pub type TreeViewOnNodeToggleCallbackType =
    extern "C" fn(RefAny, CallbackInfo, usize, bool) -> Update;
impl_widget_callback!(
    TreeViewOnNodeToggle,
    OptionTreeViewOnNodeToggle,
    TreeViewOnNodeToggleCallback,
    TreeViewOnNodeToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TreeViewOnNodeToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TREE_VIEW_ON_NODE_TOGGLE_INVOKER,
    invoker_ty:     AzTreeViewOnNodeToggleCallbackInvoker,
    thunk_fn:       az_tree_view_on_node_toggle_callback_thunk,
    setter_fn:      AzApp_setTreeViewOnNodeToggleCallbackInvoker,
    from_handle_fn: AzTreeViewOnNodeToggleCallback_createFromHostHandle,
    from_handle_byref_fn: AzTreeViewOnNodeToggleCallback_createFromHostHandleByref,
    extra_args:     [ node_index: usize, expand: bool ],
}

/// Callback invoked when a drag is DROPPED on a row: a task on a list, a
/// message on a folder, a file on a folder.
///
/// `node_index` is the row's depth-first index, exactly as for
/// [`TreeViewOnNodeClickCallbackType`]. The tree knows nothing of what is
/// dragged: the app kept it when the drag started, or reads it
/// (`CallbackInfo::get_drag_data`). With the hook set, every row accepts a
/// drag over it.
pub type TreeViewOnNodeDropCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    TreeViewOnNodeDrop,
    OptionTreeViewOnNodeDrop,
    TreeViewOnNodeDropCallback,
    TreeViewOnNodeDropCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TreeViewOnNodeDropCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TREE_VIEW_ON_NODE_DROP_INVOKER,
    invoker_ty:     AzTreeViewOnNodeDropCallbackInvoker,
    thunk_fn:       az_tree_view_on_node_drop_callback_thunk,
    setter_fn:      AzApp_setTreeViewOnNodeDropCallbackInvoker,
    from_handle_fn: AzTreeViewOnNodeDropCallback_createFromHostHandle,
    from_handle_byref_fn: AzTreeViewOnNodeDropCallback_createFromHostHandleByref,
    extra_args:     [ node_index: usize ],
}

/// The class of the tree's container and of every row: how the arrow-key
/// handler finds the tree it is in and tells rows from children containers.
const TREE_CLASS_NAME: &str = "__azul-native-tree-view";
const TREE_ROW_CLASS_NAME: &str = "__azul-native-tree-view-row";
/// The class of a parent row's disclosure box: the arrow's own click target.
const TREE_TOGGLE_CLASS_NAME: &str = "__azul-native-tree-view-toggle";
/// The class of a node's badge (the count after its label).
const TREE_BADGE_CLASS_NAME: &str = "__azul-native-tree-view-badge";
const TREE_TOGGLE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str(TREE_TOGGLE_CLASS_NAME))];
const TREE_ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(TREE_ROW_CLASS_NAME))];

// ---- R5: the parts' BASE - the structure every theme's tree shares ----
//
// A theme's part is its base below, THEN its skin (paint and metrics): the
// `TREE_..._STYLE` statics of `themes::flat` (`themes::flat::tree_view_look`),
// and `themes::flora::tree_view_look` for flora - every colour the tree paints
// is its theme's. The base comes first in every theme, so an unpinned tree
// (`TreeViewLook::of`) declares it once, outside every `@theme` block. The
// leaf spacer has no skin: `LEAF_SPACER_STYLE` is its whole style in every
// theme.

/// The tree's structure: a column of rows that scrolls when it overflows.
pub(crate) static TREE_CONTAINER_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Auto)),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
];

/// A row's structure, selected or not: icon and label side by side on one
/// midline, under the pointer.
pub(crate) static ROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// An open parent's children container: a column of rows.
pub(crate) static CHILDREN_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
];

/// The disclosure icon keeps its column's width.
pub(crate) static ICON_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// The disclosure arrow's own click box (a tree with a toggle hook): it
/// centres the arrow on the row's midline, keeps the arrow's width and takes
/// the pointer. Structure only - the same in every theme; the arrow inside
/// carries the look's icon style.
pub(crate) static TREE_TOGGLE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// The label takes the rest of the row.
pub(crate) static LABEL_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

/// A node's badge (the count after its label) keeps its width at the row's
/// end.
pub(crate) static BADGE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

// -- Leaf spacer (same width as icon, for alignment) --

pub(crate) static LEAF_SPACER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(16))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

// ============================================================================
// Data structures
// ============================================================================

/// A single node in a tree hierarchy, with optional children.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TreeViewNode {
    /// Display text for this node.
    pub label: AzString,
    /// Child nodes nested under this node.
    pub children: TreeViewNodeVec,
    /// An icon shown between the disclosure arrow and the label (a
    /// `Dom::create_icon` name: "home", "folder", "cloud"), or empty for none.
    pub icon: AzString,
    /// A short count or mark drawn after the label in the accent ("3": a
    /// mail folder's unread messages), or empty for none. It is part of the
    /// row's text, so the row's name reads "Inbox 3".
    pub badge: AzString,
    /// Whether children are visible (only meaningful when the node has
    /// children, loaded or not).
    pub is_expanded: bool,
    /// Whether this node is visually selected.
    pub is_selected: bool,
    /// The node HAS children the app has not loaded yet (a folder that was
    /// never listed): it shows the disclosure arrow like a parent, and opening
    /// it - a click on the arrow, a double-click on the row, Right on the
    /// focused row - asks the app through `on_node_toggle`, which loads the
    /// children and rebuilds the tree with them (and this flag cleared).
    pub has_unloaded_children: bool,
}

impl TreeViewNode {
    /// Creates a new collapsed, unselected leaf node with the given label.
    pub fn new<S: Into<AzString>>(label: S) -> Self {
        Self {
            label: label.into(),
            children: TreeViewNodeVec::from_const_slice(&[]),
            icon: AzString::from_const_str(""),
            badge: AzString::from_const_str(""),
            is_expanded: false,
            is_selected: false,
            has_unloaded_children: false,
        }
    }

    /// Show `icon` (a `Dom::create_icon` name) between the disclosure arrow
    /// and the label.
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// Draw `badge` after the label in the accent ("3"); empty for none.
    pub fn set_badge(&mut self, badge: AzString) {
        self.badge = badge;
    }

    /// [`Self::set_badge`] for the builder chain.
    #[must_use]
    pub fn with_badge(mut self, badge: AzString) -> Self {
        self.set_badge(badge);
        self
    }

    /// Mark the node as having children that are not loaded yet (see
    /// [`Self::has_unloaded_children`]).
    pub const fn set_unloaded_children(&mut self, unloaded: bool) {
        self.has_unloaded_children = unloaded;
    }

    /// [`Self::set_unloaded_children`] for the builder chain.
    #[must_use]
    pub const fn with_unloaded_children(mut self, unloaded: bool) -> Self {
        self.set_unloaded_children(unloaded);
        self
    }

    /// Whether the node is a parent: it has children, or children to load.
    #[must_use]
    pub fn has_children(&self) -> bool {
        self.has_unloaded_children || !self.children.as_slice().is_empty()
    }

    /// Appends a child node.
    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Builder method: appends a child node.
    #[must_use]
    pub fn with_child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    /// Builder method: sets the expanded state.
    #[must_use]
    pub const fn with_expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
        self
    }

    /// Builder method: sets the selected state.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }
}

impl_option!(
    TreeViewNode,
    OptionTreeViewNode,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    TreeViewNode,
    TreeViewNodeVec,
    TreeViewNodeVecDestructor,
    TreeViewNodeVecDestructorType,
    TreeViewNodeVecSlice,
    OptionTreeViewNode
);
impl_vec_clone!(TreeViewNode, TreeViewNodeVec, TreeViewNodeVecDestructor);
impl_vec_debug!(TreeViewNode, TreeViewNodeVec);
impl_vec_partialeq!(TreeViewNode, TreeViewNodeVec);
impl_vec_mut!(TreeViewNode, TreeViewNodeVec);

/// Hierarchical tree view widget with expandable/collapsible nodes.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TreeView {
    /// Root node of the tree hierarchy.
    pub root: TreeViewNode,
    /// Optional callback fired when any node is clicked.
    pub on_node_click: OptionTreeViewOnNodeClick,
    /// Optional callback fired when the keyboard asks to open or close a node
    /// (Right on a closed parent, Left on an open one). Without it those two
    /// keys do nothing; every other key of the tree works regardless.
    pub on_node_toggle: OptionTreeViewOnNodeToggle,
    /// Optional callback fired when a drag is dropped on a row; with it, the
    /// tree is a drop target (every row accepts a drag over it).
    pub on_node_drop: OptionTreeViewOnNodeDrop,
    /// The widget theme this tree is PINNED to (`with_theme`), or `None` to
    /// follow the app theme (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`; flat unless the app chose another).
    pub theme: crate::widgets::themes::OptionUiTheme,
}

impl TreeView {
    /// Creates a new tree view with the given root node and no click callback.
    #[must_use]
    pub fn new(root: TreeViewNode) -> Self {
        Self {
            root,
            on_node_click: None.into(),
            on_node_toggle: None.into(),
            on_node_drop: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Pin the widget theme: the tree keeps this look whatever the app theme
    /// is. Unset (`None`), it follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Sets the callback invoked when any tree node is clicked.
    pub fn set_on_node_click<C: Into<TreeViewOnNodeClickCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_node_click = Some(TreeViewOnNodeClick {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder method: sets the node-click callback.
    #[must_use]
    pub fn with_on_node_click<C: Into<TreeViewOnNodeClickCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_node_click(data, callback);
        self
    }

    /// Sets the callback invoked when the keyboard asks to open or close a node.
    pub fn set_on_node_toggle<C: Into<TreeViewOnNodeToggleCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_node_toggle = Some(TreeViewOnNodeToggle {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder method: sets the node-toggle callback.
    #[must_use]
    pub fn with_on_node_toggle<C: Into<TreeViewOnNodeToggleCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_node_toggle(data, callback);
        self
    }

    /// Sets the callback invoked when a drag is dropped on a row; the tree
    /// becomes a drop target.
    pub fn set_on_node_drop<C: Into<TreeViewOnNodeDropCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_node_drop = Some(TreeViewOnNodeDrop {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder method: sets the node-drop callback.
    #[must_use]
    pub fn with_on_node_drop<C: Into<TreeViewOnNodeDropCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_node_drop(data, callback);
        self
    }

    /// Renders the tree view into a [`Dom`] subtree.
    ///
    /// The look comes from the theme module (`themes::flat::tree_view_look` /
    /// `themes::flora::tree_view_look`); with no theme pinned every part
    /// carries both looks, each in its `@theme(<name>)` block, and the app
    /// theme picks. The rows, the roving Tab stop, the click and the arrow
    /// keys are the same in every theme.
    #[must_use]
    pub fn dom(self) -> Dom {
        const TREE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(TREE_CLASS_NAME))];

        let look = TreeViewLook::of(self.theme);
        let badge = TreeViewBadgeLook::of(self.theme);
        let root = self.root;
        // WAI-ARIA APG tree view: the tree is ONE Tab stop - the first VISIBLE
        // selected row, or the first row when none is. The arrow keys move
        // within it (`on_tree_row_key`).
        let stop = first_visible_selected(&root, &mut 0).unwrap_or(0);
        let container = look.container.clone();
        let classes = match look.marker {
            None => IdOrClassVec::from_const_slice(TREE_CLASS),
            Some(marker) => IdOrClassVec::from_vec(vec![
                Class(AzString::from_const_str(TREE_CLASS_NAME)),
                Class(AzString::from_const_str(marker)),
            ]),
        };
        let rows = RowContext {
            on_click: self.on_node_click,
            on_toggle: self.on_node_toggle,
            on_drop: self.on_node_drop,
            stop,
            look,
            badge,
        };

        let mut children = Vec::new();
        let mut index: usize = 0;
        render_rows(&root, &rows, &mut index, &mut children);

        Dom::create_div()
            .with_css_props(container)
            .with_ids_and_classes(classes)
            // The tree itself; its rows are the items (`render_rows`).
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Outline,
                ..Default::default()
            })
            .with_children(DomVec::from_vec(children))
    }
}

/// What a theme gives a tree view: one style per part, and the marker class
/// its root carries (`None` for flat, whose root carries none). The rows, the
/// roving Tab stop, the click and the arrow keys are the widget's, the same
/// in every theme; a selected row's disclosure icon and label have a style of
/// their own so a look can write them in the selection's ink.
#[derive(Debug, Clone)]
pub(crate) struct TreeViewLook {
    /// The tree's own surface.
    pub(crate) container: CssPropertyWithConditionsVec,
    /// A row.
    pub(crate) row: CssPropertyWithConditionsVec,
    /// A selected row.
    pub(crate) row_selected: CssPropertyWithConditionsVec,
    /// The container an open parent's children sit in (the indent).
    pub(crate) children: CssPropertyWithConditionsVec,
    /// The disclosure icon of a parent row.
    pub(crate) icon: CssPropertyWithConditionsVec,
    /// The disclosure icon of a selected parent row.
    pub(crate) icon_selected: CssPropertyWithConditionsVec,
    /// The empty block a leaf row keeps in the icon's place.
    pub(crate) leaf_spacer: CssPropertyWithConditionsVec,
    /// A row's label.
    pub(crate) label: CssPropertyWithConditionsVec,
    /// A selected row's label.
    pub(crate) label_selected: CssPropertyWithConditionsVec,
    /// The theme marker class on the tree's root, if the look has one.
    pub(crate) marker: Option<&'static str>,
}

impl TreeViewLook {
    /// The look `theme` pins, or - unpinned - the look that follows the app
    /// theme: every part carries both themes' declarations, each theme's in
    /// its `@theme(<name>)` block (`theme_blocks::follow_props`), and the root
    /// the marker of the theme the DOM is built for.
    pub(crate) fn of(theme: crate::widgets::themes::OptionUiTheme) -> Self {
        use crate::widgets::themes::{flat, flora, theme_blocks::follow_props, UiTheme};
        match theme.into_option() {
            Some(UiTheme::Flat) => flat::tree_view_look(),
            Some(UiTheme::Flora) => flora::tree_view_look(),
            None => {
                let (a, b) = (flat::tree_view_look(), flora::tree_view_look());
                let both = |x: &CssPropertyWithConditionsVec, y: &CssPropertyWithConditionsVec| {
                    follow_props(x.as_ref(), y.as_ref())
                };
                Self {
                    container: both(&a.container, &b.container),
                    row: both(&a.row, &b.row),
                    row_selected: both(&a.row_selected, &b.row_selected),
                    children: both(&a.children, &b.children),
                    icon: both(&a.icon, &b.icon),
                    icon_selected: both(&a.icon_selected, &b.icon_selected),
                    leaf_spacer: both(&a.leaf_spacer, &b.leaf_spacer),
                    label: both(&a.label, &b.label),
                    label_selected: both(&a.label_selected, &b.label_selected),
                    marker: match UiTheme::current() {
                        UiTheme::Flat => a.marker,
                        UiTheme::Flora => b.marker,
                    },
                }
            }
        }
    }
}

/// What a theme gives a node's BADGE (the count after its label): its style
/// on a row and on a selected row. Apart from [`TreeViewLook`] so the looks
/// grow without touching each theme's existing tree look.
#[derive(Debug, Clone)]
pub(crate) struct TreeViewBadgeLook {
    /// The badge on a row.
    pub(crate) badge: CssPropertyWithConditionsVec,
    /// The badge on a selected row.
    pub(crate) badge_selected: CssPropertyWithConditionsVec,
}

impl TreeViewBadgeLook {
    /// The badge look `theme` pins, or - unpinned - both themes' looks, each
    /// in its `@theme(<name>)` block (as [`TreeViewLook::of`]).
    pub(crate) fn of(theme: crate::widgets::themes::OptionUiTheme) -> Self {
        use crate::widgets::themes::{flat, flora, theme_blocks::follow_props, UiTheme};
        match theme.into_option() {
            Some(UiTheme::Flat) => flat::tree_view_badge_look(),
            Some(UiTheme::Flora) => flora::tree_view_badge_look(),
            None => {
                let (a, b) = (flat::tree_view_badge_look(), flora::tree_view_badge_look());
                Self {
                    badge: follow_props(a.badge.as_ref(), b.badge.as_ref()),
                    badge_selected: follow_props(
                        a.badge_selected.as_ref(),
                        b.badge_selected.as_ref(),
                    ),
                }
            }
        }
    }
}

// ============================================================================
// Internal: recursive DOM rendering
// ============================================================================

/// What every row of one tree shares while it renders.
struct RowContext {
    on_click: OptionTreeViewOnNodeClick,
    on_toggle: OptionTreeViewOnNodeToggle,
    /// The app's drop hook: with it every row is a drop target.
    on_drop: OptionTreeViewOnNodeDrop,
    /// The depth-first index of the row that holds the tree's one Tab stop.
    stop: usize,
    /// The styles every row, icon, label and children container takes.
    look: TreeViewLook,
    /// The style of a node's badge.
    badge: TreeViewBadgeLook,
}

/// The depth-first index of the first selected node a user can SEE (every
/// ancestor expanded), counting hidden nodes exactly as `render_rows` does.
fn first_visible_selected(node: &TreeViewNode, index: &mut usize) -> Option<usize> {
    let current = *index;
    *index += 1;
    if node.is_selected {
        return Some(current);
    }
    let children = node.children.as_slice();
    if children.is_empty() {
        return None;
    }
    if node.is_expanded {
        for child in children {
            if let Some(found) = first_visible_selected(child, index) {
                return Some(found);
            }
        }
    } else {
        count_descendants(children, index);
    }
    None
}

/// `render_rows` with no toggle hook, the first row as the Tab stop and the
/// flat look - the shape the rendering tests drive directly.
#[cfg(test)]
fn render_node(
    node: &TreeViewNode,
    on_click: &OptionTreeViewOnNodeClick,
    index: &mut usize,
    out: &mut Vec<Dom>,
) {
    let rows = RowContext {
        on_click: on_click.clone(),
        on_toggle: None.into(),
        on_drop: None.into(),
        stop: *index,
        look: crate::widgets::themes::flat::tree_view_look(),
        badge: crate::widgets::themes::flat::tree_view_badge_look(),
    };
    render_rows(node, &rows, index, out);
}

fn render_rows(node: &TreeViewNode, rows: &RowContext, index: &mut usize, out: &mut Vec<Dom>) {
    let current_index = *index;
    *index += 1;

    // A parent: it has children, loaded or still to load.
    let has_children = node.has_children();
    let children_loaded = !node.children.as_slice().is_empty();
    let look = &rows.look;

    // Choose the row's parts by selection state
    let (row_style, icon_style, label_style) = if node.is_selected {
        (&look.row_selected, &look.icon_selected, &look.label_selected)
    } else {
        (&look.row, &look.icon, &look.label)
    };

    // Build the disclosure icon or spacer
    let icon_or_spacer = if has_children {
        let icon_name = if node.is_expanded {
            "expand_more"
        } else {
            "chevron_right"
        };
        let arrow =
            Dom::create_icon(AzString::from_const_str(icon_name)).with_css_props(icon_style.clone());
        if rows.on_toggle.is_some() {
            // With a toggle hook the arrow is its OWN click target: a click on
            // it opens or closes the node and stops there, so the row's click
            // (a selection) does not run too - the file manager's tree.
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(TREE_TOGGLE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(TREE_TOGGLE_BASE))
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        refany: RefAny::new(ToggleData {
                            node_index: current_index,
                            is_expanded: node.is_expanded,
                            on_node_toggle: rows.on_toggle.clone(),
                        }),
                        callback: CoreCallback {
                            cb: on_tree_toggle_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    }]
                    .into(),
                )
                .with_child(arrow)
        } else {
            arrow
        }
    } else {
        // Empty spacer for leaf alignment
        Dom::create_div().with_css_props(look.leaf_spacer.clone())
    };

    // Build the label
    let label =
        crate::widgets::widget_p_with_text(node.label.clone()).with_css_props(label_style.clone());

    // The row's parts: the disclosure, the node's own icon (when it has one,
    // in the disclosure's ink and size), the label, the badge (when it has
    // one).
    let mut parts = Vec::with_capacity(4);
    parts.push(icon_or_spacer);
    if !node.icon.as_str().is_empty() {
        parts.push(Dom::create_icon(node.icon.clone()).with_css_props(icon_style.clone()));
    }
    parts.push(label);
    if !node.badge.as_str().is_empty() {
        let badge_style = if node.is_selected {
            &rows.badge.badge_selected
        } else {
            &rows.badge.badge
        };
        parts.push(
            crate::widgets::widget_p_with_text(node.badge.clone())
                .with_css_props(badge_style.clone())
                .with_class(AzString::from_const_str(TREE_BADGE_CLASS_NAME)),
        );
    }

    // Build the row: one Tab stop per tree (the roving tabindex), the arrow
    // keys on every row, the click only when the app listens for it.
    let mut row = Dom::create_div()
        .with_css_props(row_style.clone())
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TREE_ROW_CLASS))
        .with_tab_index(crate::widgets::roving::item_tab_index(
            current_index,
            rows.stop,
        ))
        // An ITEM of the tree, saying whether it is open (a parent only) and
        // whether it is selected. The tree's shape lives in the app, which
        // rebuilds on every open, close and pick, so every build publishes it
        // afresh. The NAME comes from the row's own text.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::OutlineItem,
            states: row_states(has_children, node.is_expanded, node.is_selected),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(parts));

    let mut callbacks: Vec<CoreCallbackData> = Vec::with_capacity(2);
    // The click callback, if provided - always FIRST.
    if let Some(cb) = rows.on_click.as_ref() {
        let cb_data = NodeClickData {
            node_index: current_index,
            on_node_click: Some(TreeViewOnNodeClick {
                callback: cb.callback.clone(),
                refany: cb.refany.clone(),
            })
            .into(),
        };
        callbacks.push(CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::Click),
            refany: RefAny::new(cb_data),
            callback: CoreCallback {
                cb: on_tree_node_click as usize,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        });
    }
    callbacks.push(CoreCallbackData {
        event: EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown),
        refany: RefAny::new(TreeRowData {
            node_index: current_index,
            has_children,
            children_loaded,
            is_expanded: node.is_expanded,
            on_node_toggle: rows.on_toggle.clone(),
        }),
        callback: CoreCallback {
            cb: on_tree_row_key as usize,
            ctx: azul_core::refany::OptionRefAny::None,
        },
    });
    // A double-click on a parent row opens or closes it, when the app
    // listens for that (a leaf has nothing to open).
    if has_children && rows.on_toggle.is_some() {
        callbacks.push(CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::DoubleClick),
            refany: RefAny::new(ToggleData {
                node_index: current_index,
                is_expanded: node.is_expanded,
                on_node_toggle: rows.on_toggle.clone(),
            }),
            callback: CoreCallback {
                cb: on_tree_toggle_double_click as usize,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        });
    }
    // A drop target when the app takes drops: a drag over the row is
    // accepted, a drop reports the row's node.
    if let Some(hook) = rows.on_drop.as_ref() {
        callbacks.push(CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::DragOver),
            refany: RefAny::new(DragOverData),
            callback: CoreCallback {
                cb: on_tree_row_drag_over as usize,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        });
        callbacks.push(CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::Drop),
            refany: RefAny::new(NodeDropData {
                node_index: current_index,
                on_node_drop: Some(hook.clone()).into(),
            }),
            callback: CoreCallback {
                cb: on_tree_row_drop as usize,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        });
    }
    row = row.with_callbacks(callbacks.into());

    out.push(row);

    // Render the children if expanded - the LOADED ones: an open node whose
    // children the app has not listed yet draws no container at all (an
    // empty one would sit under the row while the listing runs).
    if children_loaded && node.is_expanded {
        let mut child_doms = Vec::new();
        for child in node.children.as_slice() {
            render_rows(child, rows, index, &mut child_doms);
        }

        let children_container = Dom::create_div()
            .with_css_props(look.children.clone())
            .with_children(DomVec::from_vec(child_doms));

        out.push(children_container);
    } else if children_loaded {
        // Still count collapsed children for correct depth-first indexing
        count_descendants(node.children.as_slice(), index);
    }
}

/// What a tree row announces: open or closed when it is a parent, then
/// whether it is selected.
fn row_states(
    has_children: bool,
    is_expanded: bool,
    is_selected: bool,
) -> azul_core::a11y::AccessibilityStateVec {
    use azul_core::a11y::AccessibilityState::{Collapsed, Expanded, Selected};

    let mut states = Vec::with_capacity(2);
    if has_children {
        states.push(if is_expanded { Expanded } else { Collapsed });
    }
    if is_selected {
        states.push(Selected);
    }
    states.into()
}

/// Advance the index counter past all descendants without rendering them.
fn count_descendants(nodes: &[TreeViewNode], index: &mut usize) {
    for node in nodes {
        *index += 1;
        if !node.children.as_slice().is_empty() {
            count_descendants(node.children.as_slice(), index);
        }
    }
}

// ============================================================================
// Internal callback data
// ============================================================================

struct NodeClickData {
    node_index: usize,
    on_node_click: OptionTreeViewOnNodeClick,
}

/// What a drag over a drop-target row carries: nothing (it only accepts).
struct DragOverData;

/// What a drop on a row carries: the node, and the app's drop hook.
struct NodeDropData {
    node_index: usize,
    on_node_drop: OptionTreeViewOnNodeDrop,
}

/// A drag over a row of a tree that takes drops: accept it.
extern "C" fn on_tree_row_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

/// A drop on a row: the app hears which node it landed on.
extern "C" fn on_tree_row_drop(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut data) = data.downcast_mut::<NodeDropData>() else {
        return Update::DoNothing;
    };
    let node_index = data.node_index;
    match data.on_node_drop.as_mut() {
        Some(TreeViewOnNodeDrop { refany, callback }) => {
            callback.invoke(refany.clone(), info, node_index)
        }
        None => Update::DoNothing,
    }
}

/// What the arrow-key handler needs to know about the row it runs on.
struct TreeRowData {
    node_index: usize,
    /// A parent: children loaded, or still to load.
    has_children: bool,
    /// The children are in the tree (Right on the open row can move into
    /// them); `false` for a parent whose children the app has not loaded.
    children_loaded: bool,
    is_expanded: bool,
    on_node_toggle: OptionTreeViewOnNodeToggle,
}

/// What a click on the disclosure arrow and a double-click on a parent row
/// carry: the node, its state, and the app's toggle hook.
struct ToggleData {
    node_index: usize,
    is_expanded: bool,
    on_node_toggle: OptionTreeViewOnNodeToggle,
}

/// The toggle a click or double-click asks for: `(node, expand, hook)` -
/// open a closed node, close an open one.
fn toggle_request(data: &mut RefAny) -> Option<(usize, bool, OptionTreeViewOnNodeToggle)> {
    let d = data.downcast_ref::<ToggleData>()?;
    Some((d.node_index, !d.is_expanded, d.on_node_toggle.clone()))
}

/// A click on a parent row's disclosure arrow: asks the app to open a closed
/// node or close an open one - and stops there, so the row's own click (a
/// selection) does not run.
extern "C" fn on_tree_toggle_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((node_index, expand, on_toggle)) = toggle_request(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    toggle(&on_toggle, info, node_index, expand)
}

/// A double-click on a parent row: the same request as the arrow's click.
extern "C" fn on_tree_toggle_double_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((node_index, expand, on_toggle)) = toggle_request(&mut data) else {
        return Update::DoNothing;
    };
    toggle(&on_toggle, info, node_index, expand)
}

// ============================================================================
// Callbacks
// ============================================================================

extern "C" fn on_tree_node_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    let Some(mut refany) = refany.downcast_mut::<NodeClickData>() else {
        return Update::DoNothing;
    };

    let node_index = refany.node_index;

    match refany.on_node_click.as_mut() {
        Some(TreeViewOnNodeClick { refany, callback }) => {
            callback.invoke(refany.clone(), info, node_index)
        }
        None => Update::DoNothing,
    }
}

/// Arrow keys on the focused row (WAI-ARIA APG tree view):
///
/// * Up / Down - the previous / next VISIBLE row (a closed parent's children
///   are skipped), holding at the ends;
/// * Right - on a closed parent, ask the app to open it (`on_node_toggle`); on
///   an open one, move to its first child; on a leaf, nothing;
/// * Left - on an open parent, ask the app to close it; otherwise move to the
///   parent row (nothing on a top-level row);
/// * Home / End - the first / last visible row.
///
/// Moving focus selects nothing (Enter/Space - a click - does); the target row
/// becomes the tree's one Tab stop. Every handled key is `prevent_default`-ed,
/// including one that goes nowhere, so spatial navigation cannot walk out of
/// the tree. Any key held with Alt, Ctrl, Cmd or Shift, and every other key,
/// keeps its default.
extern "C" fn on_tree_row_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    use crate::widgets::roving;

    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    if !matches!(
        key,
        K::Up | K::Down | K::Left | K::Right | K::Home | K::End
    ) {
        return Update::DoNothing;
    }
    let (node_index, has_children, children_loaded, is_expanded, on_toggle) = {
        let Some(row) = data.downcast_ref::<TreeRowData>() else {
            return Update::DoNothing;
        };
        (
            row.node_index,
            row.has_children,
            row.children_loaded,
            row.is_expanded,
            row.on_node_toggle.clone(),
        )
    };

    let focused = info.get_hit_node();
    let Some(tree) = tree_container_of(&info, focused) else {
        return Update::DoNothing;
    };
    let mut rows = Vec::new();
    collect_visible_rows(&info, tree, &mut rows);
    let Some(current) = rows.iter().position(|n| *n == focused) else {
        return Update::DoNothing;
    };

    info.prevent_default();
    let open = has_children && is_expanded;
    let target = match key {
        K::Up => current.checked_sub(1),
        K::Down => Some(current + 1).filter(|t| *t < rows.len()),
        K::Home => Some(0),
        K::End => rows.len().checked_sub(1),
        // An open parent's first child is the next visible row - once the
        // children are in the tree (an open node still loading shows none).
        K::Right if open && children_loaded => Some(current + 1).filter(|t| *t < rows.len()),
        K::Right if open => None,
        K::Right if has_children => return toggle(&on_toggle, info, node_index, true),
        K::Left if open => return toggle(&on_toggle, info, node_index, false),
        K::Left => parent_row_of(&info, tree, focused)
            .and_then(|parent| rows.iter().position(|n| *n == parent)),
        _ => None,
    };
    let Some(target) = target.filter(|t| *t != current) else {
        return Update::DoNothing;
    };
    roving::move_stop(&mut info, &rows, target);
    Update::DoNothing
}

/// Asks the app to open (`expand`) or close node `node_index`.
fn toggle(
    on_toggle: &OptionTreeViewOnNodeToggle,
    info: CallbackInfo,
    node_index: usize,
    expand: bool,
) -> Update {
    match on_toggle.as_ref() {
        Some(TreeViewOnNodeToggle { callback, refany }) => {
            callback.invoke(refany.clone(), info, node_index, expand)
        }
        None => Update::DoNothing,
    }
}

/// The tree container `node` sits in: its nearest ancestor with the tree class.
fn tree_container_of(
    info: &CallbackInfo,
    node: azul_core::dom::DomNodeId,
) -> Option<azul_core::dom::DomNodeId> {
    let mut cur = info.get_parent(node);
    while let Some(n) = cur {
        if crate::widgets::roving::has_class(info, n, TREE_CLASS_NAME) {
            return Some(n);
        }
        cur = info.get_parent(n);
    }
    None
}

/// Every row under `parent` in document order - which is the visible order:
/// a closed parent renders no children container at all.
fn collect_visible_rows(
    info: &CallbackInfo,
    parent: azul_core::dom::DomNodeId,
    out: &mut Vec<azul_core::dom::DomNodeId>,
) {
    let mut cur = info.get_first_child(parent);
    while let Some(n) = cur {
        if crate::widgets::roving::has_class(info, n, TREE_ROW_CLASS_NAME) {
            out.push(n);
        } else {
            // A children container: its rows follow the row that owns it.
            collect_visible_rows(info, n, out);
        }
        cur = info.get_next_sibling(n);
    }
}

/// The row that owns `row`: a child row sits in a children container, which
/// directly follows its parent's row. `None` for a top-level row.
fn parent_row_of(
    info: &CallbackInfo,
    tree: azul_core::dom::DomNodeId,
    row: azul_core::dom::DomNodeId,
) -> Option<azul_core::dom::DomNodeId> {
    let container = info.get_parent(row)?;
    if container == tree {
        return None;
    }
    info.get_previous_sibling(container)
}

// ============================================================================
// Trait impls
// ============================================================================

impl From<TreeView> for Dom {
    fn from(tv: TreeView) -> Self {
        tv.dom()
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType, TabIndex},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        widgets::{
            roving::test_support as rv,
            // Flat's skins of the tree's parts (they are the theme's, in
            // `themes::flat`), under the names the assertions read.
            themes::flat::{
                TREE_CHILDREN_STYLE as CHILDREN_STYLE, TREE_CONTAINER_STYLE,
                TREE_ICON_STYLE as ICON_STYLE, TREE_LABEL_STYLE as LABEL_STYLE,
                TREE_ROW_SELECTED_STYLE as ROW_SELECTED_STYLE, TREE_ROW_STYLE as ROW_STYLE,
            },
            themes::UiTheme,
        },
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    /// `tv` in the FLAT look: the tests below compare rendered styles with
    /// the flat const slices, which an unpinned tree (following the app
    /// theme) carries inside its `@theme(flat)` block.
    fn flat_dom(tv: TreeView) -> Dom {
        tv.with_theme(UiTheme::Flat).dom()
    }

    // ------------------------------------------------------------------
    // Fixtures: trees
    // ------------------------------------------------------------------

    fn leaf(label: &str) -> TreeViewNode {
        TreeViewNode::new(label)
    }

    /// Total node count of a subtree: the node itself plus every descendant,
    /// expanded or not. This is the quantity `render_node` must advance the
    /// index counter by, whatever the expansion state.
    fn subtree_len(node: &TreeViewNode) -> usize {
        1 + node
            .children
            .as_slice()
            .iter()
            .map(subtree_len)
            .sum::<usize>()
    }

    /// A root with `n` leaf children.
    fn wide(n: usize, expanded: bool) -> TreeViewNode {
        let mut root = leaf("wide").with_expanded(expanded);
        for i in 0..n {
            root.add_child(leaf(&format!("c{i}")));
        }
        root
    }

    /// A left-spine chain `depth` nodes deep; `expanded` is applied to every
    /// level. Built bottom-up so *construction* is iterative — only the
    /// functions under test recurse.
    fn chain(depth: usize, expanded: bool) -> TreeViewNode {
        assert!(depth >= 1, "a chain has at least the root");
        let mut node = leaf("tip").with_expanded(expanded);
        for i in 1..depth {
            node = leaf(&format!("n{i}"))
                .with_child(node)
                .with_expanded(expanded);
        }
        node
    }

    /// Four levels with alternating expansion, so both `render_node` branches
    /// nest inside each other.
    fn deep_mixed() -> TreeViewNode {
        leaf("root")
            .with_expanded(true)
            .with_child(
                leaf("a")
                    .with_expanded(false) // collapsed: a1/a1x are counted, not drawn
                    .with_child(leaf("a1").with_expanded(true).with_child(leaf("a1x"))),
            )
            .with_child(
                leaf("b")
                    .with_expanded(true)
                    .with_child(leaf("b1"))
                    .with_child(leaf("b2").with_expanded(true).with_child(leaf("b2x"))),
            )
            .with_child(leaf("c").with_selected(true))
    }

    /// Every shape whose combination of branches `render_node` /
    /// `count_descendants` can take: leaves, expanded-but-childless nodes,
    /// collapsed parents, expanded parents, an expanded subtree buried under a
    /// collapsed one, and a collapsed subtree under an expanded one.
    fn shapes() -> Vec<TreeViewNode> {
        vec![
            leaf("solo"),
            leaf("solo-expanded").with_expanded(true), // expanded but childless
            leaf("solo-selected").with_selected(true),
            leaf("p").with_child(leaf("a")).with_child(leaf("b")),
            leaf("p")
                .with_child(leaf("a"))
                .with_child(leaf("b"))
                .with_expanded(true),
            leaf("p")
                .with_child(leaf("a").with_expanded(true).with_child(leaf("a1")))
                .with_expanded(true),
            leaf("p").with_child(leaf("a").with_expanded(true).with_child(leaf("a1"))),
            leaf("p")
                .with_child(leaf("a").with_child(leaf("a1")))
                .with_expanded(true),
            deep_mixed(),
            wide(64, false),
            wide(64, true),
        ]
    }

    /// Labels chosen to break naive string handling: empty, whitespace-only,
    /// embedded NUL (`AzString` is length-based, so it must not truncate),
    /// ZWJ emoji, RTL, stacked combining marks, zero-width/BOM, bidi override,
    /// control chars, and a string that looks like an icon name.
    fn pathological_labels() -> Vec<String> {
        vec![
            String::new(),
            "   ".to_string(),
            "a\u{0}b".to_string(),
            "👨‍👩‍👧‍👦".to_string(),
            "مرحبا".to_string(),
            "e\u{0301}\u{0301}\u{0301}".to_string(),
            "\u{200b}\u{feff}".to_string(),
            "\u{202e}gnirts".to_string(),
            "line\nbreak\ttab\r".to_string(),
            "chevron_right".to_string(),
            "x".repeat(100_000),
        ]
    }

    /// Runs `f` on a thread with a roomy stack. `render_node`,
    /// `count_descendants` and `TreeViewNode`'s drop glue all recurse once per
    /// tree level, and a blown stack aborts the whole test binary instead of
    /// failing one test — the explicit stack keeps the depth assertions
    /// meaningful rather than a coin flip on the harness default.
    fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(f)
            .expect("spawning the deep-recursion thread failed")
            .join()
            .expect("deep-recursion thread panicked");
    }

    // ------------------------------------------------------------------
    // Fixtures: DOM inspection
    // ------------------------------------------------------------------

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(dom: &Dom) -> Option<&str> {
        match dom.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match dom.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn icon_of(dom: &Dom) -> Option<&str> {
        match dom.root.get_node_type() {
            NodeType::Icon(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    /// True when a node's inline style is exactly the given const style slice.
    fn style_is(dom: &Dom, expected: &[CssPropertyWithConditions]) -> bool {
        *dom.root.get_style()
            == css::Css::from(CssPropertyWithConditionsVec::from_vec(expected.to_vec()))
    }

    /// Flat's part as the tree renders it (R5): the widget's `base`, then
    /// flat's const `skin`.
    fn flat_part(
        base: &[CssPropertyWithConditions],
        skin: &[CssPropertyWithConditions],
    ) -> Vec<CssPropertyWithConditions> {
        base.iter().chain(skin.iter()).cloned().collect()
    }

    /// The `(icon-or-spacer, label)` pair of a rendered row. A parent's
    /// disclosure icon sits in its click target (the toggle box); the pair
    /// names the icon itself.
    fn row_parts(row: &Dom) -> (&Dom, &Dom) {
        let ch = row.children.as_ref();
        assert_eq!(ch.len(), 2, "every row is [icon|spacer, label]");
        (disclosure_icon(&ch[0]), &ch[1])
    }

    /// The disclosure icon inside a toggle box, or the node itself (a leaf's
    /// spacer, or an icon rendered without its box).
    fn disclosure_icon(first: &Dom) -> &Dom {
        let is_toggle = first
            .root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == TREE_TOGGLE_CLASS_NAME));
        match first.children.as_ref() {
            [icon] if is_toggle => icon,
            _ => first,
        }
    }

    /// Every rendered row in `nodes`, in visual order. Rows are the only nodes
    /// `render_node` gives a tab index to; everything else at this level is a
    /// children container, which is recursed into.
    fn collect_rows<'a>(nodes: &'a [Dom], out: &mut Vec<&'a Dom>) {
        for n in nodes {
            if n.root.get_tab_index().is_some() {
                out.push(n);
            } else {
                collect_rows(n.children.as_ref(), out);
            }
        }
    }

    fn rows_of(nodes: &[Dom]) -> Vec<&Dom> {
        let mut out = Vec::new();
        collect_rows(nodes, &mut out);
        out
    }

    /// The `node_index` the row's click payload carries (`None` when the row
    /// has no callback attached).
    fn click_index_of(row: &Dom) -> Option<usize> {
        let mut data = row.root.get_callbacks().as_ref().first()?.refany.clone();
        let payload = data
            .downcast_ref::<NodeClickData>()
            .expect("a row callback payload is always a NodeClickData");
        let index = payload.node_index;
        drop(payload);
        Some(index)
    }

    /// `(node_index, label)` for every rendered row.
    fn rendered_pairs(nodes: &[Dom]) -> Vec<(usize, String)> {
        rows_of(nodes)
            .iter()
            .map(|row| {
                let (_, label) = row_parts(row);
                (
                    click_index_of(row).expect("row must carry a click payload"),
                    text_of(label)
                        .expect("a row's second child is the label text node")
                        .to_string(),
                )
            })
            .collect()
    }

    /// Independent reference model of what `render_node` should emit:
    /// pre-order over the *whole* tree, but only visible nodes produce a row.
    /// Written from the documented contract, not from the implementation.
    fn expected_pairs(node: &TreeViewNode, next: &mut usize, out: &mut Vec<(usize, String)>) {
        let index = *next;
        *next += 1;
        out.push((index, node.label.as_str().to_string()));

        let children = node.children.as_slice();
        if node.is_expanded && !children.is_empty() {
            for c in children {
                expected_pairs(c, next, out);
            }
        } else {
            // Hidden descendants still consume indices.
            *next += subtree_len(node) - 1;
        }
    }

    fn expected_of(tree: &TreeViewNode, start: usize) -> Vec<(usize, String)> {
        let mut next = start;
        let mut out = Vec::new();
        expected_pairs(tree, &mut next, &mut out);
        out
    }

    /// The true recursive descendant count — what `estimated_total_children`
    /// caches and what `convert_dom_into_compact_dom` allocates from.
    fn recursive_descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + recursive_descendants(c))
            .sum()
    }

    fn assert_estimates_consistent(dom: &Dom) {
        assert_eq!(
            dom.estimated_total_children,
            recursive_descendants(dom),
            "estimated_total_children desynced from the real subtree size"
        );
        for c in dom.children.as_ref() {
            assert_estimates_consistent(c);
        }
    }

    // ------------------------------------------------------------------
    // Fixtures: callbacks
    // ------------------------------------------------------------------

    type ClickLog = Arc<Mutex<Vec<usize>>>;

    /// Offset applied by `record_click_all_windows` so the two recorders stay
    /// distinguishable in the log.
    const SENTINEL: usize = 1_000_000;

    extern "C" fn record_click(mut data: RefAny, _info: CallbackInfo, node_index: usize) -> Update {
        if let Some(log) = data.downcast_ref::<ClickLog>() {
            log.lock().expect("click log poisoned").push(node_index);
        }
        Update::RefreshDom
    }

    /// A second callback with a *deliberately different body*: two identical
    /// `extern "C"` bodies are fair game for identical-code folding, which
    /// would merge their addresses and make "last write wins" vacuous.
    extern "C" fn record_click_all_windows(
        mut data: RefAny,
        _info: CallbackInfo,
        node_index: usize,
    ) -> Update {
        if let Some(log) = data.downcast_ref::<ClickLog>() {
            log.lock()
                .expect("click log poisoned")
                .push(node_index.wrapping_add(SENTINEL));
        }
        Update::RefreshDomAllWindows
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn cb(f: TreeViewOnNodeClickCallbackType) -> TreeViewOnNodeClickCallback {
        f.into()
    }

    fn new_log() -> ClickLog {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn entries(log: &ClickLog) -> Vec<usize> {
        log.lock().expect("click log poisoned").clone()
    }

    fn some_click(f: TreeViewOnNodeClickCallbackType, log: &ClickLog) -> OptionTreeViewOnNodeClick {
        Some(TreeViewOnNodeClick {
            callback: cb(f),
            refany: RefAny::new(log.clone()),
        })
        .into()
    }

    /// Invokes `on_tree_node_click` once per payload against one shared
    /// `CallbackInfo`. `on_tree_node_click` never touches the layout results,
    /// so an empty `LayoutWindow` is enough.
    fn run_clicks(payloads: Vec<RefAny>) -> Vec<Update> {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        payloads
            .into_iter()
            .map(|p| on_tree_node_click(p, info))
            .collect()
    }

    // ==================================================================
    // TreeViewNode::new
    // ==================================================================

    #[test]
    fn new_defaults_to_a_collapsed_unselected_childless_node() {
        let node = TreeViewNode::new("Root");

        assert_eq!(node.label.as_str(), "Root");
        assert!(
            node.children.as_slice().is_empty(),
            "a fresh node has no children"
        );
        assert_eq!(node.children.len(), 0);
        assert!(
            node.children.capacity() >= node.children.len(),
            "len must never exceed capacity"
        );
        assert!(!node.is_expanded, "a fresh node is collapsed");
        assert!(!node.is_selected, "a fresh node is unselected");
    }

    #[test]
    fn new_preserves_pathological_labels_byte_for_byte() {
        for label in pathological_labels() {
            let node = TreeViewNode::new(label.clone());
            assert_eq!(
                node.label.as_str(),
                label.as_str(),
                "label must survive verbatim"
            );
            assert_eq!(
                node.label.as_str().len(),
                label.len(),
                "an embedded NUL must not truncate the label"
            );
            // …and the state defaults must not depend on the label at all.
            assert!(!node.is_expanded);
            assert!(!node.is_selected);
            assert!(node.children.as_slice().is_empty());
        }
    }

    #[test]
    fn new_accepts_every_into_azstring_source_identically() {
        let from_str = TreeViewNode::new("same");
        let from_string = TreeViewNode::new("same".to_string());
        let from_azstring = TreeViewNode::new(AzString::from("same"));

        assert_eq!(from_str, from_string);
        assert_eq!(from_str, from_azstring);
    }

    #[test]
    fn new_with_a_megabyte_label_does_not_truncate_or_panic() {
        let huge = "λ".repeat(500_000); // 1 MB of UTF-8
        let node = TreeViewNode::new(huge.clone());
        assert_eq!(node.label.as_str().len(), huge.len());
        assert_eq!(node.label.as_str(), huge);
    }

    // ==================================================================
    // TreeViewNode::add_child / with_child
    // ==================================================================

    #[test]
    fn add_child_and_with_child_agree() {
        let mut mutated = leaf("root");
        mutated.add_child(leaf("a"));
        mutated.add_child(leaf("b"));

        let built = leaf("root").with_child(leaf("a")).with_child(leaf("b"));

        assert_eq!(
            mutated, built,
            "the builder and the mutator must produce the same node"
        );
    }

    #[test]
    fn add_child_preserves_order_duplicates_and_len_capacity_invariants() {
        let n = 5_000;
        let mut root = leaf("root");
        for i in 0..n {
            root.add_child(leaf(&format!("c{i}")));
            assert_eq!(root.children.len(), i + 1, "len must track every push");
            assert!(
                root.children.capacity() >= root.children.len(),
                "capacity must never fall below len"
            );
        }
        // Order is insertion order, and nothing is deduplicated.
        root.add_child(leaf("c0"));
        assert_eq!(
            root.children.len(),
            n + 1,
            "duplicates are kept, not merged"
        );
        assert_eq!(root.children.as_slice()[0].label.as_str(), "c0");
        assert_eq!(root.children.as_slice()[n - 1].label.as_str(), "c4999");
        assert_eq!(root.children.as_slice()[n].label.as_str(), "c0");
        assert_eq!(subtree_len(&root), n + 2);
    }

    #[test]
    fn child_vec_survives_the_borrowed_to_owned_transition() {
        // `TreeViewNode::new` seeds `children` from a *const* slice (no
        // destructor, zero capacity). The first push has to switch it to an
        // owned heap buffer; a clone taken afterwards must be fully
        // independent, or dropping either one would free the other's memory.
        let mut root = leaf("root");
        assert_eq!(root.children.capacity(), 0);

        root.add_child(leaf("a"));
        root.add_child(leaf("b"));

        let mut copy = root.clone();
        copy.add_child(leaf("c"));
        copy.children.as_mut()[0].label = AzString::from("mutated");

        assert_eq!(root.children.len(), 2, "the original must not see the push");
        assert_eq!(
            root.children.as_slice()[0].label.as_str(),
            "a",
            "the clone must own its own child storage"
        );
        assert_eq!(copy.children.len(), 3);
        assert_eq!(copy.children.as_slice()[0].label.as_str(), "mutated");

        drop(copy);
        // Original still readable after the clone is gone (no shared buffer).
        assert_eq!(root.children.as_slice()[1].label.as_str(), "b");
    }

    #[test]
    fn with_child_nests_arbitrarily_deep_without_panicking() {
        on_big_stack(|| {
            let depth = 1_000;
            let root = chain(depth, true);
            assert_eq!(subtree_len(&root), depth);

            // Deep clone + deep drop both recurse per level as well.
            let copy = root.clone();
            assert_eq!(copy, root);
            drop(copy);
            drop(root);
        });
    }

    // ==================================================================
    // TreeViewNode::with_expanded / with_selected
    // ==================================================================

    #[test]
    fn with_expanded_and_with_selected_are_orthogonal_and_idempotent() {
        for expanded in [false, true] {
            for selected in [false, true] {
                let node = leaf("n").with_expanded(expanded).with_selected(selected);
                assert_eq!(node.is_expanded, expanded);
                assert_eq!(node.is_selected, selected);

                // Order must not matter…
                let flipped = leaf("n").with_selected(selected).with_expanded(expanded);
                assert_eq!(node, flipped);

                // …and applying the same value twice must be a no-op.
                let twice = node.clone().with_expanded(expanded).with_selected(selected);
                assert_eq!(node, twice);

                // The last write wins when the value is flipped.
                let overwritten = node.clone().with_expanded(!expanded);
                assert_eq!(overwritten.is_expanded, !expanded);
                assert_eq!(
                    overwritten.is_selected, selected,
                    "with_expanded must not touch is_selected"
                );
            }
        }
    }

    #[test]
    fn state_builders_do_not_disturb_label_or_children() {
        let base = leaf("keep me").with_child(leaf("a")).with_child(leaf("b"));
        let styled = base
            .clone()
            .with_expanded(true)
            .with_selected(true)
            .with_expanded(false);

        assert_eq!(styled.label, base.label);
        assert_eq!(styled.children, base.children);
        assert!(!styled.is_expanded);
        assert!(styled.is_selected);
    }

    #[test]
    fn nodes_differing_only_in_state_are_not_equal() {
        let base = leaf("n");
        assert_ne!(base, base.clone().with_expanded(true));
        assert_ne!(base, base.clone().with_selected(true));
        assert_ne!(base, base.clone().with_child(leaf("a")));
        assert_ne!(base, leaf("m"));
    }

    #[test]
    fn equality_ignores_how_the_child_vec_was_built() {
        let pushed = leaf("root").with_child(leaf("a")).with_child(leaf("b"));
        let from_vec = TreeViewNode {
            label: AzString::from("root"),
            children: TreeViewNodeVec::from_vec(vec![leaf("a"), leaf("b")]),
            icon: AzString::from(""),
            badge: AzString::from(""),
            is_expanded: false,
            is_selected: false,
            has_unloaded_children: false,
        };
        assert_eq!(
            pushed, from_vec,
            "the vec's allocation strategy must not leak into equality"
        );
    }

    // ==================================================================
    // TreeView::new / set_on_node_click / with_on_node_click
    // ==================================================================

    #[test]
    fn treeview_new_keeps_the_root_intact_and_installs_no_callback() {
        for root in shapes() {
            let tv = TreeView::new(root.clone());
            assert_eq!(tv.root, root, "new must not rewrite the tree");
            assert!(
                tv.on_node_click.as_ref().is_none(),
                "new must not install a callback"
            );
        }
    }

    #[test]
    fn set_on_node_click_installs_then_overwrites() {
        let log = new_log();
        let mut tv = TreeView::new(leaf("root"));

        tv.set_on_node_click(RefAny::new(log.clone()), cb(record_click));
        assert!(tv.on_node_click.as_ref().is_some());

        tv.set_on_node_click(RefAny::new(log.clone()), cb(record_click_all_windows));
        let installed = tv
            .on_node_click
            .as_ref()
            .expect("a callback is still installed");
        assert_eq!(
            installed.callback,
            cb(record_click_all_windows),
            "the last write must win"
        );
        assert_ne!(installed.callback, cb(record_click));
    }

    #[test]
    fn with_on_node_click_matches_set_on_node_click() {
        // Both sides get *clones of the same* `RefAny`: `RefAny`'s equality is
        // shared-identity, so two independent `RefAny::new` calls would never
        // compare equal no matter what the builders do.
        let data = RefAny::new(new_log());
        let mut mutated = TreeView::new(leaf("root"));
        mutated.set_on_node_click(data.clone(), cb(record_click));

        let built = TreeView::new(leaf("root")).with_on_node_click(data.clone(), cb(record_click));

        assert_eq!(mutated, built);
    }

    // ==================================================================
    // count_descendants  (numeric: zero / min-max / overflow)
    // ==================================================================

    #[test]
    fn count_descendants_of_an_empty_slice_is_a_no_op_even_at_usize_max() {
        // usize has no negative domain; the adversarial extremes are 0 and MAX.
        for start in [0usize, 1, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let mut index = start;
            count_descendants(&[], &mut index);
            assert_eq!(
                index, start,
                "an empty slice must not touch the counter (and must not overflow at MAX)"
            );
        }
    }

    #[test]
    fn count_descendants_counts_every_node_regardless_of_expansion() {
        for shape in shapes() {
            let nodes = shape.children.as_slice();
            let expected: usize = nodes.iter().map(subtree_len).sum();

            for start in [0usize, 7, 1_000_000] {
                let mut index = start;
                count_descendants(nodes, &mut index);
                assert_eq!(
                    index - start,
                    expected,
                    "collapsed and expanded descendants must count the same"
                );
            }
        }
    }

    #[test]
    fn count_descendants_reaches_exactly_usize_max_without_overflowing() {
        let tree = deep_mixed();
        let nodes = tree.children.as_slice();
        let total: usize = nodes.iter().map(subtree_len).sum();

        let mut index = usize::MAX - total;
        count_descendants(nodes, &mut index);
        assert_eq!(
            index,
            usize::MAX,
            "landing exactly on usize::MAX must not overflow"
        );
    }

    #[test]
    fn count_descendants_survives_a_deep_chain() {
        on_big_stack(|| {
            let depth = 10_000;
            let root = chain(depth, false);
            let mut index = 0usize;
            count_descendants(root.children.as_slice(), &mut index);
            assert_eq!(index, depth - 1, "every hidden descendant is counted once");
        });
    }

    // ==================================================================
    // render_node  (numeric: index accounting)
    // ==================================================================

    #[test]
    fn render_node_advance_equals_subtree_size_for_every_shape() {
        // The load-bearing invariant: whether a subtree is drawn or skipped,
        // it must consume exactly one index per node — otherwise a collapsed
        // sibling shifts every later row's click index.
        for shape in shapes() {
            let expected = subtree_len(&shape);
            for start in [0usize, 1, 12_345, usize::MAX / 4] {
                let mut index = start;
                let mut out = Vec::new();
                render_node(
                    &shape,
                    &OptionTreeViewOnNodeClick::None,
                    &mut index,
                    &mut out,
                );
                assert_eq!(
                    index - start,
                    expected,
                    "index advance must equal the subtree size, expanded or not"
                );
                assert!(!out.is_empty(), "every node renders at least its own row");
            }
        }
    }

    #[test]
    fn render_node_emits_preorder_indices_for_visible_rows_only() {
        for shape in shapes() {
            let log = new_log();
            let on_click = some_click(record_click, &log);

            let mut index = 0usize;
            let mut out = Vec::new();
            render_node(&shape, &on_click, &mut index, &mut out);

            assert_eq!(
                rendered_pairs(&out),
                expected_of(&shape, 0),
                "rendered rows must match the independent pre-order model"
            );
        }
    }

    #[test]
    fn render_node_appends_and_offsets_from_a_nonzero_start_index() {
        let start = 12_345usize;
        let shape = deep_mixed();
        let log = new_log();
        let on_click = some_click(record_click, &log);

        // Pre-existing content in `out` must be preserved, not clobbered.
        let mut out = vec![
            Dom::create_div(),
            Dom::create_text_do_not_use_without_block_level_wrapper("sentinel"),
        ];
        let mut index = start;
        render_node(&shape, &on_click, &mut index, &mut out);

        assert_eq!(
            text_of(&out[1]),
            Some("sentinel"),
            "render_node must append to `out`, never rewrite it"
        );
        assert_eq!(
            rendered_pairs(&out[2..]),
            expected_of(&shape, start),
            "a non-zero start index must offset every emitted index"
        );
        assert_eq!(index, start + subtree_len(&shape));
    }

    #[test]
    fn render_node_lands_exactly_on_usize_max_without_overflowing() {
        // Three nodes, started so the *last* index handed out is usize::MAX - 1
        // and the counter finishes on usize::MAX: one node short of the cliff.
        let tree = leaf("root")
            .with_child(leaf("a"))
            .with_child(leaf("b"))
            .with_expanded(true);
        assert_eq!(subtree_len(&tree), 3);

        let log = new_log();
        let on_click = some_click(record_click, &log);

        let mut index = usize::MAX - 3;
        let mut out = Vec::new();
        render_node(&tree, &on_click, &mut index, &mut out);

        assert_eq!(index, usize::MAX, "must land exactly on MAX, not wrap");
        let indices: Vec<usize> = rows_of(&out)
            .iter()
            .filter_map(|r| click_index_of(r))
            .collect();
        assert_eq!(
            indices,
            vec![usize::MAX - 3, usize::MAX - 2, usize::MAX - 1],
            "extreme indices must be carried verbatim into the click payloads"
        );
    }

    #[cfg(all(debug_assertions, panic = "unwind"))]
    #[test]
    fn render_node_index_overflow_is_loud_not_silently_wrapped() {
        // `render_node` does an unguarded `*index += 1`. Starting at
        // usize::MAX must not quietly wrap the counter to 0 (which would give
        // two different rows the same click index); an overflow-checked build
        // has to panic instead.
        let node = leaf("boom");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut index = usize::MAX;
            let mut out = Vec::new();
            render_node(
                &node,
                &OptionTreeViewOnNodeClick::None,
                &mut index,
                &mut out,
            );
            index
        }));

        match result {
            Err(_) => {} // overflow-checked build: panicked, as required
            Ok(index) => assert_eq!(
                index, 0,
                "without overflow checks the counter must wrap cleanly, not corrupt"
            ),
        }
    }

    #[test]
    fn render_node_without_a_callback_attaches_none() {
        for shape in shapes() {
            let mut index = 0usize;
            let mut out = Vec::new();
            render_node(
                &shape,
                &OptionTreeViewOnNodeClick::None,
                &mut index,
                &mut out,
            );

            for row in rows_of(&out) {
                // The arrow-key handler is on every row; the click is opt-in.
                let cbs = row.root.get_callbacks();
                assert!(
                    cbs.as_ref()
                        .iter()
                        .all(|cb| cb.event != EventFilter::Hover(HoverEventFilter::Click)),
                    "no click callback configured => no click callback attached"
                );
                assert_eq!(cbs.as_ref().len(), 1, "only the arrow-key handler");
            }
        }
    }

    #[test]
    fn render_node_survives_a_deep_expanded_chain() {
        on_big_stack(|| {
            let depth = 800;
            let root = chain(depth, true);

            let mut index = 0usize;
            let mut out = Vec::new();
            render_node(
                &root,
                &OptionTreeViewOnNodeClick::None,
                &mut index,
                &mut out,
            );

            assert_eq!(index, depth, "one index per level");
            assert_eq!(rows_of(&out).len(), depth, "every level renders one row");
            drop(out);
        });
    }

    #[test]
    fn render_node_handles_a_wide_fanout() {
        let n = 5_000;
        let root = wide(n, true);
        let log = new_log();
        let on_click = some_click(record_click, &log);

        let mut index = 0usize;
        let mut out = Vec::new();
        render_node(&root, &on_click, &mut index, &mut out);

        assert_eq!(index, n + 1);
        assert_eq!(out.len(), 2, "an expanded parent emits [row, container]");
        assert_eq!(out[1].children.as_ref().len(), n, "every child gets a row");

        let indices: Vec<usize> = rows_of(&out)
            .iter()
            .filter_map(|r| click_index_of(r))
            .collect();
        assert_eq!(indices, (0..=n).collect::<Vec<_>>());
    }

    // ==================================================================
    // TreeView::dom
    // ==================================================================

    #[test]
    fn dom_root_carries_the_container_class_and_style() {
        let dom = flat_dom(TreeView::new(leaf("root")));

        let classes = dom.root.get_ids_and_classes();
        assert!(
            classes
                .as_ref()
                .iter()
                .any(|c| matches!(c, Class(s) if s.as_str() == "__azul-native-tree-view")),
            "the container must be findable by its widget class"
        );
        assert!(
            style_is(&dom, &flat_part(TREE_CONTAINER_BASE, TREE_CONTAINER_STYLE)),
            "the container must use the shared const style"
        );
    }

    #[test]
    fn dom_leaf_renders_a_spacer_and_no_icon() {
        let dom = flat_dom(TreeView::new(leaf("only")));
        assert_eq!(dom.children.as_ref().len(), 1, "a leaf emits just its row");

        let row = &dom.children.as_ref()[0];
        let (icon, label) = row_parts(row);
        assert_eq!(
            icon_of(icon),
            None,
            "a childless node gets no disclosure icon"
        );
        assert!(
            style_is(icon, LEAF_SPACER_STYLE),
            "the placeholder must use the leaf-spacer style so labels stay aligned"
        );
        assert_eq!(text_of(label), Some("only"));
        assert!(style_is(label, &flat_part(LABEL_BASE, LABEL_STYLE)));
    }

    #[test]
    fn dom_expanded_parent_uses_expand_more_and_emits_a_container() {
        let tree = leaf("p")
            .with_child(leaf("a"))
            .with_child(leaf("b"))
            .with_expanded(true);
        let dom = flat_dom(TreeView::new(tree));

        assert_eq!(
            dom.children.as_ref().len(),
            2,
            "an expanded parent emits [row, children container]"
        );
        let (icon, _) = row_parts(&dom.children.as_ref()[0]);
        assert_eq!(icon_of(icon), Some("expand_more"));
        assert!(style_is(icon, &flat_part(ICON_BASE, ICON_STYLE)));

        let container = &dom.children.as_ref()[1];
        assert!(style_is(container, &flat_part(CHILDREN_BASE, CHILDREN_STYLE)));
        assert_eq!(container.children.as_ref().len(), 2, "both children drawn");
    }

    #[test]
    fn dom_collapsed_parent_uses_chevron_and_draws_no_children() {
        let tree = leaf("p").with_child(leaf("a")).with_child(leaf("b"));
        let dom = TreeView::new(tree).dom();

        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "a collapsed parent must not emit a children container"
        );
        let (icon, _) = row_parts(&dom.children.as_ref()[0]);
        assert_eq!(icon_of(icon), Some("chevron_right"));
        assert_eq!(
            rows_of(dom.children.as_ref()).len(),
            1,
            "children stay hidden"
        );
    }

    #[test]
    fn dom_expanded_but_childless_node_still_renders_a_spacer() {
        // `is_expanded` is documented as meaningful only with children.
        let dom = flat_dom(TreeView::new(leaf("empty").with_expanded(true)));
        assert_eq!(dom.children.as_ref().len(), 1, "nothing to expand into");
        let (icon, _) = row_parts(&dom.children.as_ref()[0]);
        assert_eq!(icon_of(icon), None);
        assert!(style_is(icon, LEAF_SPACER_STYLE));
    }

    #[test]
    fn dom_selected_rows_use_the_selected_style() {
        let tree = leaf("p")
            .with_expanded(true)
            .with_child(leaf("a").with_selected(true))
            .with_child(leaf("b"));
        let dom = flat_dom(TreeView::new(tree));
        let rows = rows_of(dom.children.as_ref());
        assert_eq!(rows.len(), 3);

        assert!(
            style_is(rows[0], &flat_part(ROW_BASE, ROW_STYLE)),
            "unselected root uses ROW_STYLE"
        );
        assert!(
            style_is(rows[1], &flat_part(ROW_BASE, ROW_SELECTED_STYLE)),
            "the selected node must switch to the selected style"
        );
        assert!(style_is(rows[2], &flat_part(ROW_BASE, ROW_STYLE)));
        assert!(
            !style_is(rows[1], &flat_part(ROW_BASE, ROW_STYLE)),
            "the two row styles must be distinguishable"
        );
    }

    #[test]
    fn dom_keeps_estimated_total_children_consistent_for_every_shape() {
        // A stale estimate makes `convert_dom_into_compact_dom` under-allocate
        // and panic out of bounds, so this is a crash invariant, not cosmetics.
        for shape in shapes() {
            let dom = TreeView::new(shape).dom();
            assert_estimates_consistent(&dom);
        }
    }

    #[test]
    fn dom_labels_survive_the_round_trip_unchanged() {
        let labels = pathological_labels();
        let mut root = leaf("root").with_expanded(true);
        for l in &labels {
            root.add_child(TreeViewNode::new(l.clone()));
        }

        let dom = TreeView::new(root).dom();
        let rows = rows_of(dom.children.as_ref());
        assert_eq!(rows.len(), labels.len() + 1);

        let rendered: Vec<&str> = rows[1..]
            .iter()
            .map(|r| text_of(row_parts(r).1).expect("label text node"))
            .collect();
        let expected: Vec<&str> = labels.iter().map(String::as_str).collect();
        assert_eq!(rendered, expected, "labels must survive byte-for-byte");
    }

    #[test]
    fn dom_rows_are_focusable_and_carry_exactly_one_click_callback() {
        let log = new_log();
        let tv = TreeView::new(deep_mixed())
            .with_on_node_click(RefAny::new(log.clone()), cb(record_click));
        let dom = tv.dom();

        // deep_mixed selects `c`, a visible child of the open root: the tree's
        // one Tab stop. Every other row is reached with the arrow keys.
        let rows = rows_of(dom.children.as_ref());
        let stops: Vec<String> = rows
            .iter()
            .filter(|row| matches!(row.root.get_tab_index(), Some(TabIndex::Auto)))
            .map(|row| text_of(row_parts(row).1).unwrap_or_default().to_string())
            .collect();
        assert_eq!(stops, vec!["c".to_string()], "exactly one stop: the selected row");
        for row in rows {
            assert!(
                matches!(
                    row.root.get_tab_index(),
                    Some(TabIndex::Auto | TabIndex::NoKeyboardFocus)
                ),
                "every row must stay focusable"
            );
            let cbs = row.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 2, "the click and the arrow-key callback");
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::Click),
                "rows fire on mouse-up"
            );
            assert_eq!(
                cbs.as_ref()[1].event,
                EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown),
            );
        }
    }

    #[test]
    fn dom_indices_skip_collapsed_subtrees_but_stay_preorder() {
        for shape in shapes() {
            let log = new_log();
            let dom = TreeView::new(shape.clone())
                .with_on_node_click(RefAny::new(log.clone()), cb(record_click))
                .dom();

            assert_eq!(
                rendered_pairs(dom.children.as_ref()),
                expected_of(&shape, 0),
                "dom() must index nodes pre-order over the whole tree, including the collapsed \
                 ones it does not draw"
            );
        }
    }

    #[test]
    fn dom_of_an_empty_labelled_tree_does_not_panic() {
        let dom = TreeView::new(leaf("")).dom();
        let rows = rows_of(dom.children.as_ref());
        assert_eq!(rows.len(), 1);
        assert_eq!(text_of(row_parts(rows[0]).1), Some(""));
    }

    /// One node of `without_callback_payloads`: the node, its child count,
    /// its stylesheets and its callbacks as `(event, function)` pairs.
    type NodeWithoutPayloads = (
        azul_core::dom::NodeData,
        usize,
        azul_css::css::CssVec,
        Vec<(azul_core::events::EventFilter, azul_core::callbacks::CoreCallback)>,
    );

    /// A render with the callbacks' `RefAny` payloads left out: every node
    /// pre-order with its child count and stylesheets, its callbacks as
    /// `(event, function)` pairs. RefAny equality is allocation identity, and
    /// every row carries its own key-handler payload, so two renders of the
    /// same tree differ only in those.
    fn without_callback_payloads(dom: &Dom) -> Vec<NodeWithoutPayloads> {
        let mut out = Vec::new();
        let mut stack = vec![dom];
        while let Some(d) = stack.pop() {
            let mut node = d.root.clone();
            let callbacks = node
                .callbacks
                .as_ref()
                .iter()
                .map(|c| (c.event, c.callback.clone()))
                .collect();
            node.set_callbacks(Vec::new().into());
            out.push((node, d.children.as_ref().len(), d.css.clone(), callbacks));
            stack.extend(d.children.as_ref().iter().rev());
        }
        out
    }

    #[test]
    fn from_treeview_for_dom_matches_dom() {
        for shape in shapes() {
            let via_trait: Dom = TreeView::new(shape.clone()).into();
            let via_method = TreeView::new(shape).dom();
            assert_eq!(
                without_callback_payloads(&via_trait),
                without_callback_payloads(&via_method)
            );
        }
    }

    #[test]
    fn dom_survives_a_deep_expanded_chain() {
        on_big_stack(|| {
            let depth = 800;
            let dom = TreeView::new(chain(depth, true)).dom();
            assert_eq!(rows_of(dom.children.as_ref()).len(), depth);
            assert_estimates_consistent(&dom);
            drop(dom);
        });
    }

    // ==================================================================
    // on_tree_node_click
    // ==================================================================

    #[test]
    fn click_with_a_foreign_payload_returns_do_nothing() {
        // A `RefAny` of the wrong type must be rejected, not reinterpreted.
        let payloads = vec![
            RefAny::new(0usize),
            RefAny::new(String::from("not a NodeClickData")),
            RefAny::new(leaf("also not one")),
            RefAny::new(()),
        ];
        let updates = run_clicks(payloads);
        assert_eq!(
            updates,
            vec![Update::DoNothing; 4],
            "a foreign payload must be a no-op, not a panic or a wild call"
        );
    }

    #[test]
    fn click_without_a_user_callback_returns_do_nothing() {
        let payloads = vec![
            RefAny::new(NodeClickData {
                node_index: 0,
                on_node_click: OptionTreeViewOnNodeClick::None,
            }),
            RefAny::new(NodeClickData {
                node_index: usize::MAX,
                on_node_click: OptionTreeViewOnNodeClick::None,
            }),
        ];
        assert_eq!(run_clicks(payloads), vec![Update::DoNothing; 2]);
    }

    #[test]
    fn click_forwards_the_index_verbatim_including_the_extremes() {
        let log = new_log();
        let indices = vec![0usize, 1, usize::MAX / 2, usize::MAX - 1, usize::MAX];

        let payloads: Vec<RefAny> = indices
            .iter()
            .map(|i| {
                RefAny::new(NodeClickData {
                    node_index: *i,
                    on_node_click: some_click(record_click, &log),
                })
            })
            .collect();

        let updates = run_clicks(payloads);
        assert_eq!(updates, vec![Update::RefreshDom; 5]);
        assert_eq!(
            entries(&log),
            indices,
            "the node index must reach the user callback unmodified"
        );
    }

    #[test]
    fn click_propagates_the_user_update_verbatim() {
        let log = new_log();
        let payloads = vec![
            RefAny::new(NodeClickData {
                node_index: 3,
                on_node_click: some_click(record_click, &log),
            }),
            RefAny::new(NodeClickData {
                node_index: 4,
                on_node_click: some_click(record_click_all_windows, &log),
            }),
        ];

        assert_eq!(
            run_clicks(payloads),
            vec![Update::RefreshDom, Update::RefreshDomAllWindows],
            "the dispatcher must not downgrade or upgrade the user's Update"
        );
        assert_eq!(entries(&log), vec![3, 4 + SENTINEL]);
    }

    #[test]
    fn clicking_every_rendered_row_reports_its_visual_index() {
        let shape = deep_mixed();
        let log = new_log();
        let dom = TreeView::new(shape.clone())
            .with_on_node_click(RefAny::new(log.clone()), cb(record_click))
            .dom();

        let payloads: Vec<RefAny> = rows_of(dom.children.as_ref())
            .iter()
            .map(|row| {
                row.root
                    .get_callbacks()
                    .as_ref()
                    .first()
                    .expect("every row carries the click callback")
                    .refany
                    .clone()
            })
            .collect();

        let expected: Vec<usize> = expected_of(&shape, 0).into_iter().map(|(i, _)| i).collect();
        let updates = run_clicks(payloads);

        assert_eq!(updates, vec![Update::RefreshDom; expected.len()]);
        assert_eq!(
            entries(&log),
            expected,
            "clicking row N must report N's pre-order index, collapsed siblings included"
        );
    }

    // ==================================================================
    // Roving tabindex (WAI-ARIA APG tree view, P2-12)
    // ==================================================================

    /// The keyboard fixture. Visible rows, top to bottom:
    /// `root, a, b, c, c1` - `a` is collapsed over `a1`, `b` is selected.
    ///
    /// ```text
    /// root (expanded)
    /// |- a (collapsed) - a1
    /// |- b (selected)
    /// `- c (expanded) - c1
    /// ```
    fn keyboard_tree() -> TreeViewNode {
        leaf("root")
            .with_expanded(true)
            .with_child(leaf("a").with_child(leaf("a1")))
            .with_child(leaf("b").with_selected(true))
            .with_child(leaf("c").with_expanded(true).with_child(leaf("c1")))
    }

    /// A plain tab stop, the tree, another plain tab stop.
    fn tree_page(tv: TreeView) -> StyledDom {
        let stop = || Dom::create_div().with_tab_index(TabIndex::Auto);
        let page = Dom::create_div().with_children(vec![stop(), tv.dom(), stop()].into());
        StyledDom::create_from_dom(page)
    }

    fn page_node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    fn page_before() -> DomNodeId {
        page_node(1)
    }

    /// The trailing stop is the last node in document order.
    fn page_after(styled: &StyledDom) -> DomNodeId {
        page_node(styled.node_hierarchy.as_ref().len() - 1)
    }

    /// The row whose label reads `label`: the label text's grandparent
    /// (`row > <p> > text`). Looked up, never assumed from a flatten index.
    fn row_labelled(styled: &StyledDom, label: &str) -> DomNodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        for (i, nd) in styled.node_data.as_ref().iter().enumerate() {
            let NodeType::Text(s) = nd.get_node_type() else {
                continue;
            };
            if s.as_ref().as_str() != label {
                continue;
            }
            let p = hierarchy[i].parent_id().expect("a label sits in its <p>");
            let row = hierarchy[p.index()]
                .parent_id()
                .expect("a label <p> sits in its row");
            return DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(row)),
            };
        }
        panic!("no row is labelled {label:?}");
    }

    /// Presses `key` on the row labelled `label`; panics when the row has no
    /// key handler - the state of every tree row before P2-12.
    fn press_row(
        styled: &StyledDom,
        label: &str,
        key: VirtualKeyCode,
        held: &[VirtualKeyCode],
    ) -> (Update, Vec<CallbackChange>) {
        rv::press(styled, row_labelled(styled, label), key, held)
            .expect("every tree row must carry a key handler for the arrow keys")
    }

    #[test]
    fn tab_from_the_item_before_lands_on_the_selected_row_and_the_next_tab_leaves_the_tree() {
        let styled = tree_page(TreeView::new(keyboard_tree()));
        let after = page_after(&styled);
        assert_eq!(
            rv::tab_walk(&styled, Some(page_before()), true, 2),
            vec![row_labelled(&styled, "b"), after],
            "the tree is ONE tab stop: the selected row, then out",
        );
        assert_eq!(
            rv::tab_walk(&styled, Some(after), false, 2),
            vec![row_labelled(&styled, "b"), page_before()],
        );
    }

    #[test]
    fn with_no_visible_row_selected_the_first_row_is_the_tab_stop() {
        // `a1` is selected but hidden under the collapsed `a`.
        let tree = leaf("root")
            .with_expanded(true)
            .with_child(leaf("a").with_child(leaf("a1").with_selected(true)))
            .with_child(leaf("b"));
        let styled = tree_page(TreeView::new(tree));
        assert_eq!(
            rv::tab_walk(&styled, Some(page_before()), true, 2),
            vec![row_labelled(&styled, "root"), page_after(&styled)],
        );
    }

    #[test]
    fn up_and_down_move_focus_through_the_visible_rows_only() {
        use azul_core::window::VirtualKeyCode as K;

        for (from, key, to) in [
            ("root", K::Down, "a"),
            ("a", K::Down, "b"), // a1 is hidden under the collapsed a
            ("b", K::Up, "a"),
            ("c", K::Down, "c1"),
            ("c1", K::Up, "c"),
        ] {
            let styled = tree_page(TreeView::new(keyboard_tree()));
            let (update, changes) = press_row(&styled, from, key, &[]);
            assert_eq!(
                rv::focus_request(&changes),
                Some(row_labelled(&styled, to)),
                "{key:?} on {from}",
            );
            assert!(rv::prevented(&changes), "{key:?} on {from}");
            assert_eq!(update, Update::DoNothing, "moving focus selects nothing");
        }
    }

    #[test]
    fn up_on_the_first_row_and_down_on_the_last_stay_in_the_tree_and_go_nowhere() {
        use azul_core::window::VirtualKeyCode as K;

        for (from, key) in [("root", K::Up), ("c1", K::Down)] {
            let styled = tree_page(TreeView::new(keyboard_tree()));
            let (_, changes) = press_row(&styled, from, key, &[]);
            assert!(rv::prevented(&changes), "{key:?} on {from}");
            assert_eq!(rv::focus_request(&changes), None, "{key:?} on {from}");
        }
    }

    #[test]
    fn right_enters_an_open_row_and_left_climbs_to_the_parent() {
        use azul_core::window::VirtualKeyCode as K;

        for (from, key, to) in [
            ("root", K::Right, "a"), // open: Right moves to its first child
            ("c", K::Right, "c1"),
            ("c1", K::Left, "c"), // a child: Left moves to its parent
            ("b", K::Left, "root"),
            ("a", K::Left, "root"), // a closed parent is a child of root too
        ] {
            let styled = tree_page(TreeView::new(keyboard_tree()));
            let (_, changes) = press_row(&styled, from, key, &[]);
            assert_eq!(
                rv::focus_request(&changes),
                Some(row_labelled(&styled, to)),
                "{key:?} on {from}",
            );
            assert!(rv::prevented(&changes), "{key:?} on {from}");
        }
    }

    #[test]
    fn home_and_end_jump_to_the_first_and_the_last_visible_row() {
        use azul_core::window::VirtualKeyCode as K;

        for (key, to) in [(K::Home, "root"), (K::End, "c1")] {
            let styled = tree_page(TreeView::new(keyboard_tree()));
            let (_, changes) = press_row(&styled, "b", key, &[]);
            assert_eq!(
                rv::focus_request(&changes),
                Some(row_labelled(&styled, to)),
                "{key:?}",
            );
            assert!(rv::prevented(&changes));
        }
    }

    #[test]
    fn after_an_arrow_the_focused_row_is_the_trees_only_tab_stop() {
        let mut styled = tree_page(TreeView::new(keyboard_tree()));
        let (_, changes) = press_row(&styled, "b", VirtualKeyCode::End, &[]);
        rv::apply_tab_index_writes(&mut styled, &changes);
        let after = page_after(&styled);
        assert_eq!(
            rv::tab_walk(&styled, Some(page_before()), true, 2),
            vec![row_labelled(&styled, "c1"), after],
        );
    }

    #[test]
    fn modified_and_unused_keys_on_a_row_are_not_consumed() {
        use azul_core::window::VirtualKeyCode as K;

        for (key, held) in [
            (K::Down, Some(K::LAlt)),
            (K::Right, Some(K::LShift)),
            (K::Left, Some(K::LControl)),
            (K::End, Some(K::LWin)),
            (K::Tab, None),
            (K::Return, None),
            (K::PageDown, None),
        ] {
            let styled = tree_page(TreeView::new(keyboard_tree()));
            let held: Vec<K> = held.into_iter().collect();
            let (update, changes) = press_row(&styled, "b", key, &held);
            assert_eq!(update, Update::DoNothing);
            assert!(
                changes.is_empty(),
                "{held:?}+{key:?} must not be consumed: {changes:?}"
            );
        }
    }

    /// Every `(node_index, expand)` an `on_node_toggle` hears.
    type ToggleLog = Arc<Mutex<Vec<(usize, bool)>>>;

    extern "C" fn record_toggle(
        mut data: RefAny,
        _info: CallbackInfo,
        node_index: usize,
        expand: bool,
    ) -> Update {
        if let Some(log) = data.downcast_ref::<ToggleLog>() {
            log.lock()
                .expect("toggle log poisoned")
                .push((node_index, expand));
        }
        Update::RefreshDom
    }

    fn toggle_cb(f: TreeViewOnNodeToggleCallbackType) -> TreeViewOnNodeToggleCallback {
        f.into()
    }

    fn toggles(log: &ToggleLog) -> Vec<(usize, bool)> {
        log.lock().expect("toggle log poisoned").clone()
    }

    /// `keyboard_tree` whose toggles land in `log`. Depth-first indices:
    /// root 0, a 1, a1 2 (hidden), b 3, c 4, c1 5.
    fn toggling_tree(log: &ToggleLog) -> StyledDom {
        tree_page(
            TreeView::new(keyboard_tree())
                .with_on_node_toggle(RefAny::new(log.clone()), toggle_cb(record_toggle)),
        )
    }

    #[test]
    fn arrow_right_on_a_closed_row_with_children_asks_the_app_to_expand_it() {
        let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
        let styled = toggling_tree(&log);
        let (update, changes) = press_row(&styled, "a", VirtualKeyCode::Right, &[]);
        assert_eq!(toggles(&log), vec![(1, true)], "open node 1 (a)");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        assert!(rv::prevented(&changes));
        assert_eq!(rv::focus_request(&changes), None, "focus stays on the row");
    }

    #[test]
    fn arrow_left_on_an_open_row_asks_the_app_to_collapse_it() {
        for (label, index) in [("c", 4), ("root", 0)] {
            let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
            let styled = toggling_tree(&log);
            let (_, changes) = press_row(&styled, label, VirtualKeyCode::Left, &[]);
            assert_eq!(toggles(&log), vec![(index, false)], "close {label}");
            assert!(rv::prevented(&changes));
            assert_eq!(rv::focus_request(&changes), None);
        }
    }

    #[test]
    fn right_on_a_leaf_and_left_on_a_closed_top_level_row_go_nowhere() {
        let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
        let styled = toggling_tree(&log);
        let (_, changes) = press_row(&styled, "b", VirtualKeyCode::Right, &[]);
        assert!(rv::prevented(&changes), "the tree keeps the key");
        assert_eq!(rv::focus_request(&changes), None);

        let lone = tree_page(
            TreeView::new(leaf("top").with_child(leaf("hidden")))
                .with_on_node_toggle(RefAny::new(log.clone()), toggle_cb(record_toggle)),
        );
        let (_, changes) = press_row(&lone, "top", VirtualKeyCode::Left, &[]);
        assert!(rv::prevented(&changes));
        assert_eq!(rv::focus_request(&changes), None);
        assert!(toggles(&log).is_empty(), "nothing was opened or closed");
    }

    #[test]
    fn without_a_toggle_hook_right_on_a_closed_parent_stays_in_the_tree() {
        let styled = tree_page(TreeView::new(keyboard_tree()));
        let (update, changes) = press_row(&styled, "a", VirtualKeyCode::Right, &[]);
        assert_eq!(update, Update::DoNothing);
        assert!(rv::prevented(&changes));
        assert_eq!(rv::focus_request(&changes), None);
    }

    #[test]
    fn set_on_node_toggle_installs_then_overwrites_and_new_installs_none() {
        let tv = TreeView::new(leaf("root"));
        assert!(tv.on_node_toggle.is_none());

        let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
        let mut tv = tv.with_on_node_toggle(RefAny::new(log.clone()), toggle_cb(record_toggle));
        assert!(tv.on_node_toggle.is_some());
        tv.set_on_node_toggle(RefAny::new(log), toggle_cb(record_toggle));
        assert!(tv.on_node_toggle.is_some());
    }

    /// A row is a tree ITEM (each row declared the whole tree's `Outline`
    /// role) that says whether it is open - a parent only - and whether it
    /// is selected. The tree's shape lives in the app, which rebuilds on every
    /// open, close and pick, so the build publishes it afresh each time.
    #[test]
    fn tree_rows_say_whether_they_are_open_and_selected() {
        use azul_core::a11y::{
            AccessibilityRole::{Outline, OutlineItem},
            AccessibilityState::{Collapsed, Expanded, Selected},
        };

        let styled = tree_page(TreeView::new(keyboard_tree()));
        assert_eq!(
            rv::declared(&styled, page_node(2)).map(|(role, _)| role),
            Some(Outline),
            "the tree itself is the outline",
        );
        for (label, states) in [
            ("root", vec![Expanded]),
            ("a", vec![Collapsed]),
            ("b", vec![Selected]),
            ("c", vec![Expanded]),
            ("c1", Vec::new()),
        ] {
            assert_eq!(
                rv::declared(&styled, row_labelled(&styled, label)),
                Some((OutlineItem, states)),
                "row {label}",
            );
        }
    }

    // ------------------------------------------------------------------
    // A navigation tree: lazy children, node icons, the arrow as its own
    // click target, a double-click that opens (AzDrive's drives and folders)
    // ------------------------------------------------------------------

    /// This PC > Home (never listed) > ..., and Cloud (listed and OPEN, one
    /// folder - so its leaf "mail" is a rendered row).
    /// Depth-first: "This PC" 0, "Home" 1, "Cloud" 2, "mail" 3.
    fn drives_tree() -> TreeViewNode {
        leaf("This PC")
            .with_expanded(true)
            .with_icon(AzString::from("computer"))
            .with_child(
                leaf("Home")
                    .with_icon(AzString::from("home"))
                    .with_unloaded_children(true),
            )
            .with_child(
                leaf("Cloud")
                    .with_icon(AzString::from("cloud"))
                    .with_expanded(true)
                    .with_child(leaf("mail")),
            )
    }

    fn row_named<'a>(dom: &'a Dom, label: &str) -> &'a Dom {
        rows_of(dom.children.as_ref())
            .into_iter()
            .find(|row| {
                row.children
                    .as_ref()
                    .last()
                    .and_then(|l| text_of(l))
                    == Some(label)
            })
            .unwrap_or_else(|| panic!("no row is labelled {label:?}"))
    }

    fn has_event(node: &Dom, event: EventFilter) -> bool {
        node.root
            .get_callbacks()
            .as_ref()
            .iter()
            .any(|cb| cb.event == event)
    }

    #[test]
    fn a_node_whose_children_are_not_loaded_yet_shows_a_closed_disclosure_arrow() {
        let dom = flat_dom(TreeView::new(drives_tree()));
        let home = row_named(&dom, "Home");
        let parts = home.children.as_ref();
        assert_eq!(
            icon_of(disclosure_icon(&parts[0])),
            Some("chevron_right"),
            "a folder that was never listed can still be opened"
        );
        let states = home
            .root
            .get_accessibility_info()
            .map(|i| i.states.as_ref().to_vec())
            .unwrap_or_default();
        assert!(
            states.contains(&azul_core::a11y::AccessibilityState::Collapsed),
            "and says it is closed: {states:?}"
        );
    }

    #[test]
    fn an_open_node_without_loaded_children_points_down_and_draws_no_children() {
        let tree = leaf("Home")
            .with_unloaded_children(true)
            .with_expanded(true);
        let dom = flat_dom(TreeView::new(tree));
        assert_eq!(dom.children.as_ref().len(), 1, "nothing loaded to draw");
        let icon = disclosure_icon(&dom.children.as_ref()[0].children.as_ref()[0]);
        assert_eq!(icon_of(icon), Some("expand_more"));
    }

    #[test]
    fn a_node_icon_sits_between_the_disclosure_and_the_label() {
        let dom = flat_dom(TreeView::new(drives_tree()));
        for (label, icon) in [("This PC", "computer"), ("Home", "home"), ("Cloud", "cloud")] {
            let parts = row_named(&dom, label).children.as_ref();
            assert_eq!(parts.len(), 3, "{label}: arrow, icon, label");
            assert_eq!(icon_of(&parts[1]), Some(icon), "{label}");
            assert_eq!(text_of(&parts[2]), Some(label));
        }
        // A node without an icon keeps the two-part row.
        let plain = flat_dom(TreeView::new(leaf("plain")));
        assert_eq!(plain.children.as_ref()[0].children.as_ref().len(), 2);
    }

    #[test]
    fn with_a_toggle_hook_the_arrow_is_its_own_click_target_and_a_double_click_opens() {
        let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
        let dom = flat_dom(
            TreeView::new(drives_tree())
                .with_on_node_toggle(RefAny::new(log.clone()), toggle_cb(record_toggle)),
        );
        let click = EventFilter::Hover(HoverEventFilter::Click);
        let double = EventFilter::Hover(HoverEventFilter::DoubleClick);
        for label in ["This PC", "Home", "Cloud"] {
            let row = row_named(&dom, label);
            let arrow = &row.children.as_ref()[0];
            assert!(
                has_event(arrow, click),
                "{label}: a click on the arrow opens or closes, it does not select"
            );
            assert!(has_event(row, double), "{label}: a double-click opens");
        }
        let leaf_row = row_named(&dom, "mail");
        assert!(!has_event(leaf_row, double), "a leaf has nothing to open");

        // Without the hook the arrow is part of the row (a click selects).
        let bare = flat_dom(TreeView::new(drives_tree()));
        let arrow = &row_named(&bare, "Home").children.as_ref()[0];
        assert!(!has_event(arrow, click));
    }

    #[test]
    fn a_click_on_the_arrow_asks_to_open_the_node_and_stops_there() {
        let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
        let styled = tree_page(
            TreeView::new(drives_tree())
                .with_on_node_toggle(RefAny::new(log.clone()), toggle_cb(record_toggle)),
        );
        let home = row_labelled(&styled, "Home");
        let arrow = page_node(home.node.into_crate_internal().expect("a row").index() + 1);
        let (update, changes) = rv::fire(&styled, arrow, EventFilter::Hover(HoverEventFilter::Click))
            .expect("the arrow carries its click handler");
        assert_eq!(toggles(&log), vec![(1, true)], "open node 1 (Home)");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, CallbackChange::StopPropagation)),
            "the row's own click (select) does not run too"
        );
    }

    #[test]
    fn a_double_click_on_an_open_row_asks_to_close_it() {
        let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
        let styled = tree_page(
            TreeView::new(drives_tree())
                .with_on_node_toggle(RefAny::new(log.clone()), toggle_cb(record_toggle)),
        );
        let _ = rv::fire(
            &styled,
            row_labelled(&styled, "This PC"),
            EventFilter::Hover(HoverEventFilter::DoubleClick),
        )
        .expect("a parent row carries a double-click handler");
        assert_eq!(toggles(&log), vec![(0, false)], "close node 0 (This PC)");
    }

    #[test]
    fn right_on_a_closed_node_with_unloaded_children_asks_to_open_it() {
        let log: ToggleLog = Arc::new(Mutex::new(Vec::new()));
        let styled = tree_page(
            TreeView::new(drives_tree())
                .with_on_node_toggle(RefAny::new(log.clone()), toggle_cb(record_toggle)),
        );
        let (_, changes) = press_row(&styled, "Home", VirtualKeyCode::Right, &[]);
        assert_eq!(toggles(&log), vec![(1, true)], "open node 1 (Home) to load it");
        assert!(rv::prevented(&changes));
    }
}

/// The tree's two looks (W5b). Flat is the established field; flora is a
/// sheet of field paper in a hairline, rows that wash to `--fl-hov` under the
/// pointer and ring on focus, and the selection cut as the sunken accent
/// stone in its own ink. Whichever look draws it, the rows, the roving Tab
/// stop and the accessibility tree are the tree's.
#[cfg(test)]
mod theme_tests {
    use azul_core::dom::Dom;
    use azul_css::{
        dynamic_selector::PseudoStateType,
        props::{basic::color::ColorU, property::CssPropertyType, style::StyleBackgroundContent},
    };

    use super::*;
    use crate::widgets::themes::{flora, theme_checks as tc, OptionUiTheme, UiTheme};

    const FLORA: &str = "__azul-theme-flora";

    extern "C" fn pick(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }

    /// `Root` open over `Picked` (selected, a closed parent) and `Plain` (a
    /// leaf): `root/0` is Root's row (`/0` its icon, `/1` its label),
    /// `root/1` the children container, `root/1/0` Picked's row and
    /// `root/1/1` Plain's.
    fn tree() -> TreeViewNode {
        TreeViewNode::new("Root")
            .with_expanded(true)
            .with_child(
                TreeViewNode::new("Picked")
                    .with_selected(true)
                    .with_child(TreeViewNode::new("Inner")),
            )
            .with_child(TreeViewNode::new("Plain"))
    }

    fn built(theme: UiTheme) -> Dom {
        TreeView::new(tree())
            .with_on_node_click(RefAny::new(()), pick as TreeViewOnNodeClickCallbackType)
            .with_theme(theme)
            .dom()
    }

    fn at<'a>(dom: &'a Dom, path: &[usize]) -> &'a Dom {
        path.iter().fold(dom, |node, i| &node.children.as_ref()[*i])
    }

    fn fill(node: &Dom, dark: bool) -> Option<ColorU> {
        tc::background(node, dark).as_ref().and_then(tc::bg_color)
    }

    fn layers(
        node: &Dom,
        dark: bool,
        state: Option<PseudoStateType>,
    ) -> Vec<StyleBackgroundContent> {
        tc::resolve(node, CssPropertyType::BackgroundContent, dark, state)
            .map(|p| tc::bg_layers(&p))
            .unwrap_or_default()
    }

    #[test]
    fn a_tree_without_a_theme_follows_the_app_theme_and_set_theme_pins_it() {
        let tv = TreeView::new(tree());
        assert_eq!(tv.theme, OptionUiTheme::None, "a fresh tree follows the app");
        let mut pinned = tv.clone();
        pinned.set_theme(UiTheme::Flora);
        assert_eq!(pinned.theme, OptionUiTheme::Some(UiTheme::Flora));
        assert_eq!(pinned, tv.with_theme(UiTheme::Flora), "set_theme and with_theme agree");
    }

    #[test]
    fn a_flora_tree_is_a_sheet_of_field_paper_in_a_hairline() {
        let dom = built(UiTheme::Flora);
        assert!(tc::has_class(&dom, FLORA), "the root carries flora's marker");
        assert!(tc::has_class(&dom, TREE_CLASS_NAME), "and stays findable as a tree");
        assert!(!tc::has_class(&built(UiTheme::Flat), FLORA));
        for (dark, paper, rule, ink, icon) in [
            (
                false,
                flora::LIGHT_FLD,
                flora::LIGHT_BD2,
                flora::LIGHT_INK,
                flora::LIGHT_ICON,
            ),
            (
                true,
                flora::DARK_FLD,
                flora::DARK_BD2,
                flora::DARK_INK,
                flora::DARK_ICON,
            ),
        ] {
            assert_eq!(fill(&dom, dark), Some(paper), "dark={dark}: the sheet");
            assert_eq!(
                tc::border_top_color(&dom, dark, None),
                Some(rule),
                "dark={dark}: the hairline"
            );
            assert_eq!(
                tc::text_color(at(&dom, &[0, 1]), dark),
                Some(ink),
                "dark={dark}: a label"
            );
            assert_eq!(
                tc::text_color(at(&dom, &[0, 0]), dark),
                Some(icon),
                "dark={dark}: a disclosure icon is flora's icon ink"
            );
        }
    }

    #[test]
    fn a_flora_selected_row_is_the_accent_stone_written_in_its_own_ink() {
        let dom = built(UiTheme::Flora);
        let picked = at(&dom, &[1, 0]);
        for dark in [false, true] {
            assert_eq!(
                layers(picked, dark, None),
                flora::selected_stone(),
                "dark={dark}: the selection is the sunken stone, its own colour in both modes"
            );
            assert_eq!(
                tc::text_color(at(&dom, &[1, 0, 1]), dark),
                Some(flora::LIGHT_ON_ACC),
                "dark={dark}: the selected label"
            );
            assert_eq!(
                tc::text_color(at(&dom, &[1, 0, 0]), dark),
                Some(flora::LIGHT_ON_ACC),
                "dark={dark}: the selected row's disclosure icon"
            );
            assert_ne!(
                layers(at(&dom, &[1, 1]), dark, None),
                flora::selected_stone(),
                "dark={dark}: an unselected row is not the stone"
            );
        }
    }

    #[test]
    fn flora_rows_wash_under_the_pointer_and_ring_on_focus_in_both_modes() {
        let dom = built(UiTheme::Flora);
        let plain = at(&dom, &[1, 1]);
        for dark in [false, true] {
            assert_ne!(
                layers(plain, dark, Some(PseudoStateType::Hover)),
                layers(plain, dark, None),
                "dark={dark}: a row answers the pointer"
            );
            let rows: [&[usize]; 3] = [&[0], &[1, 0], &[1, 1]];
            for path in rows {
                assert!(
                    tc::has_focus_ring(at(&dom, path), dark),
                    "dark={dark}: row {path:?} shows no focus ring"
                );
            }
        }
        tc::assert_theme_invariants("flora tree", &dom);
    }

    #[test]
    fn both_looks_build_the_same_rows_tab_stop_and_accessibility_tree() {
        let (flat, flora) = (built(UiTheme::Flat), built(UiTheme::Flora));
        let (a, b) = (tc::nodes(&flat), tc::nodes(&flora));
        assert_eq!(a.len(), b.len(), "the same tree of nodes");
        assert_eq!(tc::a11y_outline(&flat), tc::a11y_outline(&flora));
        for ((path, x), (_, y)) in a.iter().zip(b.iter()) {
            assert_eq!(
                x.root.get_callbacks().as_ref().len(),
                y.root.get_callbacks().as_ref().len(),
                "{path}: the same click and arrow-key handlers"
            );
        }
        assert_ne!(
            tc::background(&flat, false),
            tc::background(&flora, false),
            "the two looks are two looks"
        );
    }

    /// Every colour the flat tree paints is one flat's theme module declares
    /// (THEME12): the tree carried ten colours of its own in its widget file
    /// - a #FCFCFC field, a neutral grey for its chevrons, a blue of its own
    /// for its counts - none of them Office 2010's. Pinned, and following the
    /// app theme, alike.
    #[test]
    fn every_colour_the_flat_tree_paints_is_one_of_flats() {
        use crate::widgets::themes::theme_blocks::checks::under;
        let nodes = || {
            tree()
                .with_child(TreeViewNode::new("Inbox").with_badge(AzString::from("3")))
                .with_child(
                    TreeViewNode::new("Sent")
                        .with_badge(AzString::from("1"))
                        .with_selected(true),
                )
        };
        for dom in [
            TreeView::new(nodes()).with_theme(UiTheme::Flat).dom(),
            under(UiTheme::Flat, || TreeView::new(nodes()).dom()),
        ] {
            let foreign = tc::foreign_colours(&dom, UiTheme::Flat);
            assert!(
                foreign.is_empty(),
                "the flat tree paints colours flat does not have:\n  {}",
                foreign.join("\n  ")
            );
        }
    }
}

/// R5: a tree's STRUCTURE (display, flex, overflow, cursor, ...) is its base
/// - declared once, outside every `@theme(<name>)` block, so it holds under
/// flat, flora and any theme to come. What a theme owns is its skin: paint
/// and metrics.
#[cfg(test)]
mod structure_tests {
    use super::{TreeView, TreeViewNode};
    use crate::widgets::themes::{
        theme_blocks::checks::{under, BOTH},
        theme_checks::assert_structure_is_shared,
    };

    /// Every row a tree draws: an open parent (its children container), a
    /// closed one, leaves, a selected leaf and a selected parent.
    fn tree() -> TreeViewNode {
        TreeViewNode::new("Library")
            .with_expanded(true)
            .with_child(
                TreeViewNode::new("Books")
                    .with_expanded(true)
                    .with_child(TreeViewNode::new("Dune").with_selected(true))
                    .with_child(TreeViewNode::new("Emma")),
            )
            .with_child(
                TreeViewNode::new("Music")
                    .with_selected(true)
                    .with_child(TreeViewNode::new("Bach")),
            )
            .with_child(TreeViewNode::new("Notes"))
    }

    #[test]
    fn a_tree_view_declares_its_structure_once_for_every_theme() {
        for t in BOTH {
            let dom = under(t, || TreeView::new(tree()).dom());
            assert_structure_is_shared(&format!("tree view, built for {}", t.name()), &dom, &[]);
        }
    }
}

/// A node's BADGE: a short count after its label (a mail folder's unread
/// messages, Outlook's "Inbox 3"), in the accent so it reads as a count and
/// not as part of the name; on a selected row in the selection's ink.
#[cfg(test)]
mod badge_tests {
    use azul_core::dom::{Dom, NodeType};

    use super::*;
    use crate::widgets::themes::{theme_checks as tc, UiTheme};

    extern "C" fn pick(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }

    /// `ada@example.org` open over `Inbox` (badge "3"), `Drafts` (no badge)
    /// and `Sent` (selected, badge "1"): `root/0` is the account's row,
    /// `root/1` the children container, `root/1/0..2` the folders' rows.
    fn folders() -> TreeViewNode {
        TreeViewNode::new("ada@example.org")
            .with_expanded(true)
            .with_child(TreeViewNode::new("Inbox").with_badge(AzString::from("3")))
            .with_child(TreeViewNode::new("Drafts"))
            .with_child(
                TreeViewNode::new("Sent")
                    .with_selected(true)
                    .with_badge(AzString::from("1")),
            )
    }

    fn built(theme: UiTheme) -> Dom {
        TreeView::new(folders())
            .with_on_node_click(RefAny::new(()), pick as TreeViewOnNodeClickCallbackType)
            .with_theme(theme)
            .dom()
    }

    fn at<'a>(dom: &'a Dom, path: &[usize]) -> &'a Dom {
        path.iter().fold(dom, |node, i| &node.children.as_ref()[*i])
    }

    /// Every text under `node`, in document order.
    fn texts(node: &Dom) -> Vec<String> {
        let mut out = Vec::new();
        if let NodeType::Text(t) = node.root.get_node_type() {
            out.push(t.as_ref().as_str().to_string());
        }
        for child in node.children.as_ref() {
            out.extend(texts(child));
        }
        out
    }

    #[test]
    fn a_node_with_a_badge_shows_it_after_its_label() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let dom = built(theme);
            assert_eq!(
                texts(at(&dom, &[1, 0])),
                vec!["Inbox".to_string(), "3".to_string()],
                "{theme:?}: the count follows the name"
            );
            assert_eq!(
                texts(at(&dom, &[1, 2])),
                vec!["Sent".to_string(), "1".to_string()],
                "{theme:?}: a selected row keeps its count"
            );
        }
    }

    #[test]
    fn a_node_without_a_badge_draws_no_badge_node() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let dom = built(theme);
            let drafts = at(&dom, &[1, 1]);
            assert_eq!(texts(drafts), vec!["Drafts".to_string()]);
            assert_eq!(
                drafts.children.as_ref().len(),
                2,
                "{theme:?}: the leaf spacer and the label, nothing else"
            );
        }
    }

    #[test]
    fn the_badge_is_written_in_the_accent_and_on_the_selection_in_its_ink() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let dom = built(theme);
            let inbox = at(&dom, &[1, 0]);
            assert_eq!(inbox.children.as_ref().len(), 3, "{theme:?}: spacer, label, badge");
            let (label, badge) = (at(inbox, &[1]), at(inbox, &[2]));
            assert!(
                tc::has_class(badge, TREE_BADGE_CLASS_NAME),
                "{theme:?}: the badge is findable by its class"
            );
            for dark in [false, true] {
                let ink = tc::text_color(badge, dark);
                assert!(ink.is_some(), "{theme:?} dark={dark}: the badge has its own ink");
                assert_ne!(
                    ink,
                    tc::text_color(label, dark),
                    "{theme:?} dark={dark}: the count is not written like the name"
                );
            }
            let sent = at(&dom, &[1, 2]);
            assert_eq!(sent.children.as_ref().len(), 3, "{theme:?}: spacer, label, badge");
            for dark in [false, true] {
                assert_eq!(
                    tc::text_color(at(sent, &[2]), dark),
                    tc::text_color(at(sent, &[1]), dark),
                    "{theme:?} dark={dark}: on the selection the count takes the label's ink"
                );
            }
        }
    }

    #[test]
    fn set_badge_and_with_badge_agree() {
        let mut a = TreeViewNode::new("Inbox");
        a.set_badge(AzString::from("12"));
        assert_eq!(a, TreeViewNode::new("Inbox").with_badge(AzString::from("12")));
        assert_eq!(a.badge.as_str(), "12");
        assert_eq!(TreeViewNode::new("Inbox").badge.as_str(), "", "no badge by default");
    }
}

/// A tree with a drop hook (`with_on_node_drop`) is a drop target: every row
/// accepts a drag over it and reports the node a drop lands on - a task
/// dropped on a list, a message on a folder, a file on a folder.
#[cfg(test)]
mod drop_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        callbacks::Update,
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, NodeType},
        refany::RefAny,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::{TreeView, TreeViewNode, TreeViewOnNodeDropCallbackType, TREE_ROW_CLASS_NAME};
    use crate::{
        callbacks::{CallbackChange, CallbackInfo},
        widgets::{roving::test_support as rv, themes::UiTheme},
    };

    type Dropped = Arc<Mutex<Vec<usize>>>;

    extern "C" fn record_drop(mut data: RefAny, _info: CallbackInfo, node: usize) -> Update {
        if let Some(log) = data.downcast_ref::<Dropped>() {
            log.lock().expect("drop log").push(node);
        }
        Update::RefreshDom
    }

    /// Lists > Work, Azlin > Design: depth-first 0, 1, 2, 3.
    fn tree() -> TreeViewNode {
        TreeViewNode::new("Lists")
            .with_expanded(true)
            .with_child(TreeViewNode::new("Work"))
            .with_child(
                TreeViewNode::new("Azlin")
                    .with_expanded(true)
                    .with_child(TreeViewNode::new("Design")),
            )
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
                let is_row = data[n.index()]
                    .get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == TREE_ROW_CLASS_NAME));
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

    fn droppable(log: &Dropped) -> StyledDom {
        StyledDom::create_from_dom(
            TreeView::new(tree())
                .with_theme(UiTheme::Flat)
                .with_on_node_drop(RefAny::new(log.clone()), record_drop as TreeViewOnNodeDropCallbackType)
                .dom(),
        )
    }

    #[test]
    fn a_drop_on_a_row_reports_the_rows_node() {
        let log: Dropped = Arc::new(Mutex::new(Vec::new()));
        let styled = droppable(&log);
        let (update, _) = rv::fire(&styled, row(&styled, "Design"), EventFilter::Hover(HoverEventFilter::Drop))
            .expect("the row takes drops");
        assert_eq!(update, Update::RefreshDom, "the app's answer is the drop's");
        rv::fire(&styled, row(&styled, "Work"), EventFilter::Hover(HoverEventFilter::Drop))
            .expect("every row takes drops");
        assert_eq!(*log.lock().expect("drop log"), vec![3, 1]);
    }

    #[test]
    fn a_drag_over_a_row_is_accepted() {
        let log: Dropped = Arc::new(Mutex::new(Vec::new()));
        let styled = droppable(&log);
        let (_, changes) = rv::fire(&styled, row(&styled, "Azlin"), EventFilter::Hover(HoverEventFilter::DragOver))
            .expect("a drag over a row");
        assert!(changes.iter().any(|c| matches!(c, CallbackChange::AcceptDrop)));
        assert!(log.lock().expect("drop log").is_empty(), "a drag over is no drop");
    }

    #[test]
    fn a_tree_without_a_drop_hook_takes_no_drops() {
        let styled = StyledDom::create_from_dom(TreeView::new(tree()).with_theme(UiTheme::Flat).dom());
        assert!(rv::fire(&styled, row(&styled, "Work"), EventFilter::Hover(HoverEventFilter::DragOver)).is_none());
        assert!(rv::fire(&styled, row(&styled, "Work"), EventFilter::Hover(HoverEventFilter::Drop)).is_none());
    }
}
