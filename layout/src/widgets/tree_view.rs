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

use crate::callbacks::{Callback, CallbackInfo};

// -- Callback type via macro --

pub type TreeViewOnNodeClickCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    TreeViewOnNodeClick,
    OptionTreeViewOnNodeClick,
    TreeViewOnNodeClickCallback,
    TreeViewOnNodeClickCallbackType
);

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

#[derive(Debug, Clone, PartialEq)]
pub struct TreeViewNode {
    pub label: AzString,
    pub children: Vec<TreeViewNode>,
    pub is_expanded: bool,
    pub is_selected: bool,
}

impl TreeViewNode {
    pub fn new<S: Into<AzString>>(label: S) -> Self {
        Self {
            label: label.into(),
            children: Vec::new(),
            is_expanded: false,
            is_selected: false,
        }
    }

    pub fn add_child(&mut self, child: TreeViewNode) {
        self.children.push(child);
    }

    pub fn with_child(mut self, child: TreeViewNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeView {
    pub root: TreeViewNode,
    pub on_node_click: OptionTreeViewOnNodeClick,
}

impl TreeView {
    pub fn new(root: TreeViewNode) -> Self {
        Self {
            root,
            on_node_click: None.into(),
        }
    }

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

    pub fn with_on_node_click<C: Into<TreeViewOnNodeClickCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_node_click(data, callback);
        self
    }

    pub fn dom(self) -> Dom {
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

    let has_children = !node.children.is_empty();

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
    if let Some(ref cb) = on_click.as_ref() {
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
        for child in &node.children {
            render_node(child, on_click, index, &mut child_doms);
        }

        let children_container = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(CHILDREN_STYLE))
            .with_children(DomVec::from_vec(child_doms));

        out.push(children_container);
    } else if has_children {
        // Still count collapsed children for correct depth-first indexing
        count_descendants(&node.children, index);
    }
}

/// Advance the index counter past all descendants without rendering them.
fn count_descendants(nodes: &[TreeViewNode], index: &mut usize) {
    for node in nodes {
        *index += 1;
        if !node.children.is_empty() {
            count_descendants(&node.children, index);
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
    let mut refany = match refany.downcast_mut::<NodeClickData>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let node_index = refany.node_index;

    match refany.on_node_click.as_mut() {
        Some(TreeViewOnNodeClick { refany, callback }) => {
            (callback.cb)(refany.clone(), info.clone(), node_index)
        }
        None => Update::DoNothing,
    }
}

// ============================================================================
// Trait impls
// ============================================================================

impl From<TreeView> for Dom {
    fn from(tv: TreeView) -> Dom {
        tv.dom()
    }
}
