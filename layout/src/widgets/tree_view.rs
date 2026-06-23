//! Tree view widget with expandable/collapsible nodes.
//!
//! Provides [`TreeView`] and [`TreeViewNode`] for building hierarchical
//! tree structures with click callbacks and recursive DOM rendering.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorU, ColorOrSystem},
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::CssProperty,
        style::*,
    },
    *,
};

use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut};

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
    extra_args:     [ node_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- Colors --

const TEXT_COLOR: ColorU = ColorU { r: 30, g: 30, b: 30, a: 255 };
const SELECTED_BG: ColorU = ColorU { r: 0, g: 120, b: 215, a: 255 };
const SELECTED_TEXT: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
const HOVER_BG: ColorU = ColorU { r: 229, g: 243, b: 255, a: 255 };
const ICON_COLOR: ColorU = ColorU { r: 100, g: 100, b: 100, a: 255 };

// -- Tree container style --

static TREE_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor { inner: TEXT_COLOR })),
];

// -- Row style (each tree node row) --

static ROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    // Hover
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(HOVER_BG)]),
    )),
];

// -- Selected row style --
// NOTE: Intentionally duplicates base properties from ROW_STYLE because
// const-slice styling does not support runtime composition. If you change
// padding/layout in ROW_STYLE, update ROW_SELECTED_STYLE to match.

static ROW_SELECTED_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(SELECTED_BG)]),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor { inner: SELECTED_TEXT })),
];

// -- Children container style --

static CHILDREN_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(16))),
];

// -- Disclosure icon style --
// NOTE: Icon font-size (16px) must match LEAF_SPACER_STYLE width so that
// leaf nodes align with parent nodes that have a disclosure icon.

static ICON_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(16))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor { inner: ICON_COLOR })),
];

// -- Leaf spacer (same width as icon, for alignment) --

static LEAF_SPACER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(16))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

// -- Label style --

static LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(4))),
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
    /// Whether children are visible (only meaningful when `children` is non-empty).
    pub is_expanded: bool,
    /// Whether this node is visually selected.
    pub is_selected: bool,
}

impl TreeViewNode {
    /// Creates a new collapsed, unselected leaf node with the given label.
    pub fn new<S: Into<AzString>>(label: S) -> Self {
        Self {
            label: label.into(),
            children: TreeViewNodeVec::from_const_slice(&[]),
            is_expanded: false,
            is_selected: false,
        }
    }

    /// Appends a child node.
    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Builder method: appends a child node.
    #[must_use] pub fn with_child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    /// Builder method: sets the expanded state.
    #[must_use] pub const fn with_expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
        self
    }

    /// Builder method: sets the selected state.
    #[must_use] pub const fn with_selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }
}

impl_option!(TreeViewNode, OptionTreeViewNode, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(TreeViewNode, TreeViewNodeVec, TreeViewNodeVecDestructor, TreeViewNodeVecDestructorType, TreeViewNodeVecSlice, OptionTreeViewNode);
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
}

impl TreeView {
    /// Creates a new tree view with the given root node and no click callback.
    #[must_use] pub fn new(root: TreeViewNode) -> Self {
        Self {
            root,
            on_node_click: None.into(),
        }
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
    pub fn with_on_node_click<C: Into<TreeViewOnNodeClickCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_node_click(data, callback);
        self
    }

    /// Renders the tree view into a [`Dom`] subtree.
    #[must_use] pub fn dom(self) -> Dom {
        let on_node_click = self.on_node_click;
        let root = self.root;

        const TREE_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-tree-view"))];

        let mut children = Vec::new();
        let mut index: usize = 0;
        render_node(&root, &on_node_click, &mut index, &mut children);

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(TREE_CONTAINER_STYLE))
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TREE_CLASS))
            .with_children(DomVec::from_vec(children))
    }
}

// ============================================================================
// Internal: recursive DOM rendering
// ============================================================================

fn render_node(
    node: &TreeViewNode,
    on_click: &OptionTreeViewOnNodeClick,
    index: &mut usize,
    out: &mut Vec<Dom>,
) {
    let current_index = *index;
    *index += 1;

    let has_children = !node.children.as_slice().is_empty();

    // Choose row style based on selection state
    let row_style = if node.is_selected {
        ROW_SELECTED_STYLE
    } else {
        ROW_STYLE
    };

    // Build the disclosure icon or spacer
    let icon_or_spacer = if has_children {
        let icon_name = if node.is_expanded {
            "expand_more"
        } else {
            "chevron_right"
        };
        Dom::create_icon(AzString::from_const_str(icon_name))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(ICON_STYLE))
    } else {
        // Empty spacer for leaf alignment
        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(LEAF_SPACER_STYLE))
    };

    // Build the label
    let label = Dom::create_text(node.label.clone())
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE));

    // Build the row with click callback
    let mut row = Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(row_style))
        .with_tab_index(TabIndex::Auto)
        .with_children(DomVec::from_vec(vec![icon_or_spacer, label]));

    // Attach click callback if provided
    if let Some(cb) = on_click.as_ref() {
        let cb_data = NodeClickData {
            node_index: current_index,
            on_node_click: Some(TreeViewOnNodeClick {
                callback: cb.callback.clone(),
                refany: cb.refany.clone(),
            })
            .into(),
        };
        row = row.with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                refany: RefAny::new(cb_data),
                callback: CoreCallback {
                    cb: on_tree_node_click as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            }]
            .into(),
        );
    }

    out.push(row);

    // Render children if expanded
    if has_children && node.is_expanded {
        let mut child_doms = Vec::new();
        for child in node.children.as_slice() {
            render_node(child, on_click, index, &mut child_doms);
        }

        let children_container = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(CHILDREN_STYLE))
            .with_children(DomVec::from_vec(child_doms));

        out.push(children_container);
    } else if has_children {
        // Still count collapsed children for correct depth-first indexing
        count_descendants(node.children.as_slice(), index);
    }
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
            (callback.cb)(refany.clone(), info, node_index)
        }
        None => Update::DoNothing,
    }
}

// ============================================================================
// Trait impls
// ============================================================================

impl From<TreeView> for Dom {
    fn from(tv: TreeView) -> Self {
        tv.dom()
    }
}
