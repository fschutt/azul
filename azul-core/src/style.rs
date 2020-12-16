//! DOM tree to CSS style tree cascading

use std::collections::{BTreeMap, BTreeSet};
use azul_css::{
    Css, CssContentGroup, CssPath, RectStyle, RectLayout, CssProperty,
    CssPathSelector, CssPathPseudoSelector, CssNthChildSelector::*,
};
use crate::{
    dom::{DomId, NodeData},
    id_tree::{NodeId, NodeHierarchy, NodeDataContainer},
    callbacks::HitTestItem,
};

/// Has all the necessary information about the style CSS path
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct HtmlCascadeInfo {
    pub index_in_parent: u32,
    pub is_last_child: bool,
    pub is_hovered_over: bool,
    pub is_focused: bool,
    pub is_active: bool,
}

/// Returns if the style CSS path matches the DOM node (i.e. if the DOM node should be styled by that element)
pub fn matches_html_element(
    css_path: &CssPath,
    node_id: NodeId,
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<NodeData>,
    html_node_tree: &NodeDataContainer<HtmlCascadeInfo>,
) -> bool {

    use self::CssGroupSplitReason::*;

    if css_path.selectors.is_empty() {
        return false;
    }

    let mut current_node = Some(node_id);
    let mut direct_parent_has_to_match = false;
    let mut last_selector_matched = true;

    for (content_group, reason) in CssGroupIterator::new(css_path.selectors.as_ref()) {
        let cur_node_id = match current_node {
            Some(c) => c,
            None => {
                // The node has no parent, but the CSS path
                // still has an extra limitation - only valid if the
                // next content group is a "*" element
                return *content_group == [&CssPathSelector::Global];
            },
        };
        let current_selector_matches = selector_group_matches(&content_group, &html_node_tree[cur_node_id], &node_data[cur_node_id]);

        if direct_parent_has_to_match && !current_selector_matches {
            // If the element was a ">" element and the current,
            // direct parent does not match, return false
            return false; // not executed (maybe this is the bug)
        }

        // If the current selector matches, but the previous one didn't,
        // that means that the CSS path chain is broken and therefore doesn't match the element
        if current_selector_matches && !last_selector_matched {
            return false;
        }

        // Important: Set if the current selector has matched the element
        last_selector_matched = current_selector_matches;
        // Select if the next content group has to exactly match or if it can potentially be skipped
        direct_parent_has_to_match = reason == DirectChildren;
        current_node = node_hierarchy[cur_node_id].parent;
    }

    last_selector_matched
}

pub struct CssGroupIterator<'a> {
    pub css_path: &'a [CssPathSelector],
    pub current_idx: usize,
    pub last_reason: CssGroupSplitReason,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CssGroupSplitReason {
    Children,
    DirectChildren,
}

impl<'a> CssGroupIterator<'a> {
    pub fn new(css_path: &'a [CssPathSelector]) -> Self {
        let initial_len = css_path.len();
        Self {
            css_path,
            current_idx: initial_len,
            last_reason: CssGroupSplitReason::Children,
        }
    }
}

impl<'a> Iterator for CssGroupIterator<'a> {
    type Item = (CssContentGroup<'a>, CssGroupSplitReason);

    fn next(&mut self) -> Option<(CssContentGroup<'a>, CssGroupSplitReason)> {
        use self::CssPathSelector::*;

        let mut new_idx = self.current_idx;

        if new_idx == 0 {
            return None;
        }

        let mut current_path = Vec::new();

        while new_idx != 0 {
            match self.css_path.get(new_idx - 1)? {
                Children => {
                    self.last_reason = CssGroupSplitReason::Children;
                    break;
                },
                DirectChildren => {
                    self.last_reason = CssGroupSplitReason::DirectChildren;
                    break;
                },
                other => current_path.push(other),
            }
            new_idx -= 1;
        }

        // NOTE: Order inside of a ContentGroup is not important
        // for matching elements, only important for testing
        #[cfg(test)]
        current_path.reverse();

        if new_idx == 0 {
            if current_path.is_empty() {
                None
            } else {
                // Last element of path
                self.current_idx = 0;
                Some((current_path, self.last_reason))
            }
        } else {
            // skip the "Children | DirectChildren" element itself
            self.current_idx = new_idx - 1;
            Some((current_path, self.last_reason))
        }
    }
}

pub fn construct_html_cascade_tree(
    node_hierarchy: &NodeHierarchy,
    node_depths_sorted: &[(usize, NodeId)],
    focused_item: Option<NodeId>,
    hovered_items: &BTreeMap<NodeId, HitTestItem>,
    is_mouse_down: bool
) -> NodeDataContainer<HtmlCascadeInfo> {

    let mut nodes = (0..node_hierarchy.len()).map(|_| HtmlCascadeInfo {
        index_in_parent: 0,
        is_last_child: false,
        is_hovered_over: false,
        is_active: false,
        is_focused: false,
    }).collect::<Vec<_>>();

    for (_depth, parent_id) in node_depths_sorted {

        // Note: :nth-child() starts at 1 instead of 0
        let index_in_parent = parent_id.preceding_siblings(node_hierarchy).count();

        let is_parent_hovered_over = hovered_items.contains_key(parent_id);
        let parent_html_matcher = HtmlCascadeInfo {
            index_in_parent: index_in_parent as u32, // necessary for nth-child
            is_last_child: node_hierarchy[*parent_id].next_sibling.is_none(), // Necessary for :last selectors
            is_hovered_over: is_parent_hovered_over,
            is_active: is_parent_hovered_over && is_mouse_down,
            is_focused: focused_item == Some(*parent_id),
        };

        nodes[parent_id.index()] = parent_html_matcher;

        for (child_idx, child_id) in parent_id.children(node_hierarchy).enumerate() {
            let is_child_hovered_over = hovered_items.contains_key(&child_id);
            let child_html_matcher = HtmlCascadeInfo {
                index_in_parent: child_idx as u32 + 1, // necessary for nth-child
                is_last_child: node_hierarchy[child_id].next_sibling.is_none(),
                is_hovered_over: is_child_hovered_over,
                is_active: is_child_hovered_over && is_mouse_down,
                is_focused: focused_item == Some(child_id),
            };

            nodes[child_id.index()] = child_html_matcher;
        }
    }

    NodeDataContainer { internal: nodes }
}

/// Returns all CSS paths that have a `:hover` or `:active` in their path
/// (since they need to have tags for hit-testing)
pub fn collect_hover_groups(css: &Css) -> BTreeMap<CssPath, HoverGroup> {
    use azul_css::{CssPathSelector::*, CssPathPseudoSelector::*};

    let hover_rule = PseudoSelector(Hover);
    let active_rule = PseudoSelector(Active);

    // Filter out all :hover and :active rules, since we need to create tags
    // for them after the main CSS styling has been done
    css.rules().filter_map(|rule_block| {
        let pos = rule_block.path.selectors.iter().position(|x| *x == hover_rule || *x == active_rule)?;
        if rule_block.declarations.is_empty() {
            return None;
        }

        let active_or_hover = match rule_block.path.selectors.get(pos)? {
            PseudoSelector(Hover) => ActiveHover::Hover,
            PseudoSelector(Active) => ActiveHover::Active,
            _ => return None,
        };

        let css_path = CssPath { selectors: rule_block.path.selectors.iter().cloned().take(pos).collect() };
        let hover_group = HoverGroup {
            affects_layout: rule_block.declarations.iter().any(|hover_rule| hover_rule.can_trigger_relayout()),
            active_or_hover,
        };
        Some((css_path, hover_group))
    }).collect()
}

/// In order to figure out on which nodes to insert the :hover and :active hit-test tags,
/// we need to select all items that have a :hover or :active tag.
fn match_hover_selectors(
    hover_selectors: BTreeMap<CssPath, HoverGroup>,
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<NodeData>,
    html_node_tree: &NodeDataContainer<HtmlCascadeInfo>,
) -> BTreeMap<NodeId, HoverGroup> {

    let mut btree_map = BTreeMap::new();

    for (css_path, hover_selector) in hover_selectors {
        btree_map.extend(
            html_node_tree
            .linear_iter()
            .filter(|node_id| matches_html_element(&css_path, *node_id, node_hierarchy, node_data, html_node_tree))
            .map(|node_id| (node_id, hover_selector))
        );
    }

    btree_map
}

/// Matches a single group of items, panics on Children or DirectChildren selectors
///
/// The intent is to "split" the CSS path into groups by selectors, then store and cache
/// whether the direct or any parent has matched the path correctly
pub fn selector_group_matches(
    selectors: &[&CssPathSelector],
    html_node: &HtmlCascadeInfo,
    node_data: &NodeData,
) -> bool {

    use self::CssPathSelector::*;

    for selector in selectors {
        match selector {
            Global => { },
            Type(t) => {
                if node_data.get_node_type().get_path() != *t {
                    return false;
                }
            },
            Class(c) => {
                if !node_data.get_classes().iter().any(|class| class == c) {
                    return false;
                }
            },
            Id(id) => {
                if !node_data.get_ids().iter().any(|html_id| html_id == id) {
                    return false;
                }
            },
            PseudoSelector(CssPathPseudoSelector::First) => {
                // Notice: index_in_parent is 1-indexed
                if html_node.index_in_parent != 1 { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Last) => {
                // Notice: index_in_parent is 1-indexed
                if !html_node.is_last_child { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::NthChild(x)) => {
                use azul_css::CssNthChildPattern;
                match *x {
                    Number(value) => if html_node.index_in_parent != value { return false; },
                    Even => if html_node.index_in_parent % 2 == 0 { return false; },
                    Odd => if html_node.index_in_parent % 2 == 1 { return false; },
                    Pattern(CssNthChildPattern { repeat, offset }) => {
                        if html_node.index_in_parent >= offset &&
                           ((html_node.index_in_parent - offset) % repeat != 0) {
                            return false;
                        }
                    },
                }
            },
            PseudoSelector(CssPathPseudoSelector::Hover) => {
                if !html_node.is_hovered_over { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Active) => {
                if !html_node.is_active { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Focus) => {
                if !html_node.is_focused { return false; }
            },
            DirectChildren | Children => {
                panic!("Unreachable: DirectChildren or Children in CSS path!");
            },
        }
    }

    true
}

pub fn apply_style_property(style: &mut RectStyle, layout: &mut RectLayout, property: &CssProperty) {

    use azul_css::CssProperty::*;

    match property {

        Display(d)                      => layout.display = Some(*d),
        Float(f)                        => layout.float = Some(*f),
        BoxSizing(bs)                   => layout.box_sizing = Some(*bs),

        TextColor(c)                    => style.text_color = Some(*c),
        FontSize(fs)                    => style.font_size = Some(*fs),
        FontFamily(ff)                  => style.font_family = Some(ff.clone()),
        TextAlign(ta)                   => style.text_align = Some(*ta),

        LetterSpacing(ls)               => style.letter_spacing = Some(*ls),
        LineHeight(lh)                  => style.line_height = Some(*lh),
        WordSpacing(ws)                 => style.word_spacing = Some(*ws),
        TabWidth(tw)                    => style.tab_width = Some(*tw),
        Cursor(c)                       => style.cursor = Some(*c),

        Width(w)                        => layout.width = Some(*w),
        Height(h)                       => layout.height = Some(*h),
        MinWidth(mw)                    => layout.min_width = Some(*mw),
        MinHeight(mh)                   => layout.min_height = Some(*mh),
        MaxWidth(mw)                    => layout.max_width = Some(*mw),
        MaxHeight(mh)                   => layout.max_height = Some(*mh),

        Position(p)                     => layout.position = Some(*p),
        Top(t)                          => layout.top = Some(*t),
        Bottom(b)                       => layout.bottom = Some(*b),
        Right(r)                        => layout.right = Some(*r),
        Left(l)                         => layout.left = Some(*l),

        FlexWrap(fw)                    => layout.wrap = Some(*fw),
        FlexDirection(fd)               => layout.direction = Some(*fd),
        FlexGrow(fg)                    => layout.flex_grow = Some(*fg),
        FlexShrink(fs)                  => layout.flex_shrink = Some(*fs),
        JustifyContent(jc)              => layout.justify_content = Some(*jc),
        AlignItems(ai)                  => layout.align_items = Some(*ai),
        AlignContent(ac)                => layout.align_content = Some(*ac),

        BackgroundContent(bc)           => style.background = Some(bc.clone()),
        BackgroundPosition(bp)          => style.background_position = Some(*bp),
        BackgroundSize(bs)              => style.background_size = Some(*bs),
        BackgroundRepeat(br)            => style.background_repeat = Some(*br),

        OverflowX(ox)                   => layout.overflow_x = Some(*ox),
        OverflowY(oy)                   => layout.overflow_y = Some(*oy),

        PaddingTop(pt)                  => layout.padding_top = Some(*pt),
        PaddingLeft(pl)                 => layout.padding_left = Some(*pl),
        PaddingRight(pr)                => layout.padding_right = Some(*pr),
        PaddingBottom(pb)               => layout.padding_bottom = Some(*pb),

        MarginTop(mt)                   => layout.margin_top = Some(*mt),
        MarginLeft(ml)                  => layout.margin_left = Some(*ml),
        MarginRight(mr)                 => layout.margin_right = Some(*mr),
        MarginBottom(mb)                => layout.margin_bottom = Some(*mb),

        BorderTopLeftRadius(btl)        => style.border_top_left_radius = Some(*btl),
        BorderTopRightRadius(btr)       => style.border_top_right_radius = Some(*btr),
        BorderBottomLeftRadius(bbl)     => style.border_bottom_left_radius = Some(*bbl),
        BorderBottomRightRadius(bbr)    => style.border_bottom_right_radius = Some(*bbr),

        BorderTopColor(btc)             => style.border_top_color = Some(*btc),
        BorderRightColor(brc)           => style.border_right_color = Some(*brc),
        BorderLeftColor(blc)            => style.border_left_color = Some(*blc),
        BorderBottomColor(bbc)          => style.border_bottom_color = Some(*bbc),

        BorderTopStyle(bts)             => style.border_top_style = Some(*bts),
        BorderRightStyle(brs)           => style.border_right_style = Some(*brs),
        BorderLeftStyle(bls)            => style.border_left_style = Some(*bls),
        BorderBottomStyle(bbs)          => style.border_bottom_style = Some(*bbs),

        BorderTopWidth(btw)             => layout.border_top_width = Some(*btw),
        BorderRightWidth(brw)           => layout.border_right_width = Some(*brw),
        BorderLeftWidth(blw)            => layout.border_left_width = Some(*blw),
        BorderBottomWidth(bbw)          => layout.border_bottom_width = Some(*bbw),

        BoxShadowLeft(bsl)              => style.box_shadow_left = Some(*bsl),
        BoxShadowRight(bsr)             => style.box_shadow_right = Some(*bsr),
        BoxShadowTop(bst)               => style.box_shadow_top = Some(*bst),
        BoxShadowBottom(bsb)            => style.box_shadow_bottom = Some(*bsb),
    }
}

#[test]
fn test_case_issue_93() {

    use azul_css::CssPathSelector::*;
    use azul_css::*;
    use crate::dom::*;

    fn render_tab() -> Dom {
        Dom::div().with_class("tabwidget-tab")
            .with_child(Dom::label("").with_class("tabwidget-tab-label"))
            .with_child(Dom::label("").with_class("tabwidget-tab-close"))
    }

    let dom = Dom::div().with_id("editor-rooms")
    .with_child(
        Dom::div().with_class("tabwidget-bar")
        .with_child(render_tab().with_class("active"))
        .with_child(render_tab())
        .with_child(render_tab())
        .with_child(render_tab())
    );

    let dom = convert_dom_into_compact_dom(dom);

    let tab_active_close = CssPath { selectors: vec![
        Class("tabwidget-tab".to_string().into()),
        Class("active".to_string().into()),
        Children,
        Class("tabwidget-tab-close".to_string().into())
    ].into() };

    let node_hierarchy = &dom.arena.node_hierarchy;
    let node_data = &dom.arena.node_data;
    let nodes_sorted: Vec<_> = node_hierarchy.get_parents_sorted_by_depth();
    let html_node_tree = construct_html_cascade_tree(
        &node_hierarchy,
        &nodes_sorted,
        None,
        &BTreeMap::new(),
        false,
    );

    //  rules: [
    //    ".tabwidget-tab-label"                        : ColorU::BLACK,
    //    ".tabwidget-tab.active .tabwidget-tab-label"  : ColorU::WHITE,
    //    ".tabwidget-tab.active .tabwidget-tab-close"  : ColorU::RED,
    //  ]

    //  0: [div #editor-rooms ]
    //   |-- 1: [div  .tabwidget-bar]
    //   |    |-- 2: [div  .tabwidget-tab .active]
    //   |    |    |-- 3: [p  .tabwidget-tab-label]
    //   |    |    |-- 4: [p  .tabwidget-tab-close]
    //   |    |-- 5: [div  .tabwidget-tab]
    //   |    |    |-- 6: [p  .tabwidget-tab-label]
    //   |    |    |-- 7: [p  .tabwidget-tab-close]
    //   |    |-- 8: [div  .tabwidget-tab]
    //   |    |    |-- 9: [p  .tabwidget-tab-label]
    //   |    |    |-- 10: [p  .tabwidget-tab-close]
    //   |    |-- 11: [div  .tabwidget-tab]
    //   |    |    |-- 12: [p  .tabwidget-tab-label]
    //   |    |    |-- 13: [p  .tabwidget-tab-close]

    // Test 1:
    // ".tabwidget-tab.active .tabwidget-tab-label"
    // should not match
    // ".tabwidget-tab.active .tabwidget-tab-close"
    assert_eq!(matches_html_element(&tab_active_close, NodeId::new(3), &node_hierarchy, &node_data, &html_node_tree), false);

    // Test 2:
    // ".tabwidget-tab.active .tabwidget-tab-close"
    // should match
    // ".tabwidget-tab.active .tabwidget-tab-close"
    assert_eq!(matches_html_element(&tab_active_close, NodeId::new(4), &node_hierarchy, &node_data, &html_node_tree), true);
}

#[test]
fn test_css_group_iterator() {
    use self::CssPathSelector::*;
    use azul_css::*;

    // ".hello > #id_text.new_class div.content"
    // -> ["div.content", "#id_text.new_class", ".hello"]
    let selectors = vec![
        Class("hello".to_string().into()),
        DirectChildren,
        Id("id_test".to_string().into()),
        Class("new_class".to_string().into()),
        Children,
        Type(NodeTypePath::Div),
        Class("content".to_string().into()),
    ];

    let mut it = CssGroupIterator::new(&selectors);

    assert_eq!(it.next(), Some((vec![
       &Type(NodeTypePath::Div),
       &Class("content".to_string().into()),
    ], CssGroupSplitReason::Children)));

    assert_eq!(it.next(), Some((vec![
       &Id("id_test".to_string().into()),
       &Class("new_class".to_string().into()),
    ], CssGroupSplitReason::DirectChildren)));

    assert_eq!(it.next(), Some((vec![
        &Class("hello".into()),
    ], CssGroupSplitReason::DirectChildren))); // technically not correct

    assert_eq!(it.next(), None);

    // Test single class
    let selectors_2 = vec![
        Class("content".to_string().into()),
    ];

    let mut it = CssGroupIterator::new(&selectors_2);

    assert_eq!(it.next(), Some((vec![
       &Class("content".to_string().into()),
    ], CssGroupSplitReason::Children)));

    assert_eq!(it.next(), None);
}