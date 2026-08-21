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
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
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
    #[must_use]
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
        const TREE_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-tree-view"))];

        let on_node_click = self.on_node_click;
        let root = self.root;

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
    let label = Dom::create_p_with_text(node.label.clone())
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE));

    // Build the row with click callback
    let mut row = Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(row_style))
        .with_tab_index(TabIndex::Auto)
            // Role so the accessibility tree knows what this IS:
            // a hierarchy, so level and expansion can be reported. The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Outline,
                ..Default::default()
            })
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

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

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
    fn style_is(dom: &Dom, expected: &'static [CssPropertyWithConditions]) -> bool {
        *dom.root.get_style()
            == css::Css::from(CssPropertyWithConditionsVec::from_const_slice(expected))
    }

    /// The `(icon-or-spacer, label)` pair of a rendered row.
    fn row_parts(row: &Dom) -> (&Dom, &Dom) {
        let ch = row.children.as_ref();
        assert_eq!(ch.len(), 2, "every row is [icon|spacer, label]");
        (&ch[0], &ch[1])
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
            ctx: OptionRefAny::None,
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
        assert_eq!(root.children.len(), n + 1, "duplicates are kept, not merged");
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
                let twice = node
                    .clone()
                    .with_expanded(expanded)
                    .with_selected(selected);
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
            is_expanded: false,
            is_selected: false,
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
                render_node(&shape, &OptionTreeViewOnNodeClick::None, &mut index, &mut out);
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
        let mut out = vec![Dom::create_div(), Dom::create_text_do_not_use_without_block_level_wrapper("sentinel")];
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
            render_node(&node, &OptionTreeViewOnNodeClick::None, &mut index, &mut out);
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
            render_node(&shape, &OptionTreeViewOnNodeClick::None, &mut index, &mut out);

            for row in rows_of(&out) {
                assert!(
                    row.root.get_callbacks().as_ref().is_empty(),
                    "no callback configured => no callback attached"
                );
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
            render_node(&root, &OptionTreeViewOnNodeClick::None, &mut index, &mut out);

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
        let dom = TreeView::new(leaf("root")).dom();

        let classes = dom.root.get_ids_and_classes();
        assert!(
            classes
                .as_ref()
                .iter()
                .any(|c| matches!(c, Class(s) if s.as_str() == "__azul-native-tree-view")),
            "the container must be findable by its widget class"
        );
        assert!(
            style_is(&dom, TREE_CONTAINER_STYLE),
            "the container must use the shared const style"
        );
    }

    #[test]
    fn dom_leaf_renders_a_spacer_and_no_icon() {
        let dom = TreeView::new(leaf("only")).dom();
        assert_eq!(dom.children.as_ref().len(), 1, "a leaf emits just its row");

        let row = &dom.children.as_ref()[0];
        let (icon, label) = row_parts(row);
        assert_eq!(icon_of(icon), None, "a childless node gets no disclosure icon");
        assert!(
            style_is(icon, LEAF_SPACER_STYLE),
            "the placeholder must use the leaf-spacer style so labels stay aligned"
        );
        assert_eq!(text_of(label), Some("only"));
        assert!(style_is(label, LABEL_STYLE));
    }

    #[test]
    fn dom_expanded_parent_uses_expand_more_and_emits_a_container() {
        let tree = leaf("p")
            .with_child(leaf("a"))
            .with_child(leaf("b"))
            .with_expanded(true);
        let dom = TreeView::new(tree).dom();

        assert_eq!(
            dom.children.as_ref().len(),
            2,
            "an expanded parent emits [row, children container]"
        );
        let (icon, _) = row_parts(&dom.children.as_ref()[0]);
        assert_eq!(icon_of(icon), Some("expand_more"));
        assert!(style_is(icon, ICON_STYLE));

        let container = &dom.children.as_ref()[1];
        assert!(style_is(container, CHILDREN_STYLE));
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
        assert_eq!(rows_of(dom.children.as_ref()).len(), 1, "children stay hidden");
    }

    #[test]
    fn dom_expanded_but_childless_node_still_renders_a_spacer() {
        // `is_expanded` is documented as meaningful only with children.
        let dom = TreeView::new(leaf("empty").with_expanded(true)).dom();
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
        let dom = TreeView::new(tree).dom();
        let rows = rows_of(dom.children.as_ref());
        assert_eq!(rows.len(), 3);

        assert!(style_is(rows[0], ROW_STYLE), "unselected root uses ROW_STYLE");
        assert!(
            style_is(rows[1], ROW_SELECTED_STYLE),
            "the selected node must switch to the selected style"
        );
        assert!(style_is(rows[2], ROW_STYLE));
        assert!(
            !style_is(rows[1], ROW_STYLE),
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

        for row in rows_of(dom.children.as_ref()) {
            assert!(
                matches!(row.root.get_tab_index(), Some(TabIndex::Auto)),
                "every row must be keyboard focusable"
            );
            let cbs = row.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 1, "exactly one click callback per row");
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp),
                "rows fire on mouse-up"
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
                "dom() must index nodes pre-order over the whole tree, \
                 including the collapsed ones it does not draw"
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

    #[test]
    fn from_treeview_for_dom_matches_dom() {
        for shape in shapes() {
            let via_trait: Dom = TreeView::new(shape.clone()).into();
            let via_method = TreeView::new(shape).dom();
            assert_eq!(via_trait, via_method);
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
}
