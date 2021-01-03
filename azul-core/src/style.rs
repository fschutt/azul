//! DOM tree to CSS style tree cascading

use azul_css::{
    CssContentGroup, CssPath, RectStyle, RectLayout, CssProperty,
    CssPathSelector, CssPathPseudoSelector, CssNthChildSelector::*,
};
use crate::{
    dom::NodeData,
    styled_dom::{AzNode, StyledNodeState},
    id_tree::{NodeId, NodeHierarchyRef, NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut},
};

/// Has all the necessary information about the style CSS path
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct HtmlCascadeInfo {
    pub index_in_parent: u32,
    pub is_last_child: bool,
}

impl_vec!(HtmlCascadeInfo, HtmlCascadeInfoVec);
impl_vec_debug!(HtmlCascadeInfo, HtmlCascadeInfoVec);
impl_vec_partialord!(HtmlCascadeInfo, HtmlCascadeInfoVec);
impl_vec_clone!(HtmlCascadeInfo, HtmlCascadeInfoVec);
impl_vec_partialeq!(HtmlCascadeInfo, HtmlCascadeInfoVec);

impl HtmlCascadeInfoVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, HtmlCascadeInfo> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, HtmlCascadeInfo> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }
}

/// Returns if the style CSS path matches the DOM node (i.e. if the DOM node should be styled by that element)
pub(crate) fn matches_html_element(
    css_path: &CssPath,
    node_id: NodeId,
    node_hierarchy: &NodeDataContainerRef<AzNode>,
    node_data: &NodeDataContainerRef<NodeData>,
    html_node_tree: &NodeDataContainerRef<HtmlCascadeInfo>,
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
        current_node = node_hierarchy[cur_node_id].parent_id();
    }

    last_selector_matched
}

pub(crate) struct CssGroupIterator<'a> {
    pub css_path: &'a [CssPathSelector],
    pub current_idx: usize,
    pub last_reason: CssGroupSplitReason,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum CssGroupSplitReason {
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

pub(crate) fn construct_html_cascade_tree(node_hierarchy: &NodeHierarchyRef, node_depths_sorted: &[(usize, NodeId)]) -> NodeDataContainer<HtmlCascadeInfo> {

    let mut nodes = (0..node_hierarchy.len()).map(|_| HtmlCascadeInfo {
        index_in_parent: 0,
        is_last_child: false,

    }).collect::<Vec<_>>();

    for (_depth, parent_id) in node_depths_sorted {

        // Note: :nth-child() starts at 1 instead of 0
        let index_in_parent = parent_id.preceding_siblings(node_hierarchy).count();

        let parent_html_matcher = HtmlCascadeInfo {
            index_in_parent: index_in_parent as u32, // necessary for nth-child
            is_last_child: node_hierarchy[*parent_id].next_sibling.is_none(), // Necessary for :last selectors
        };

        nodes[parent_id.index()] = parent_html_matcher;

        for (child_idx, child_id) in parent_id.children(node_hierarchy).enumerate() {
            let child_html_matcher = HtmlCascadeInfo {
                index_in_parent: child_idx as u32 + 1, // necessary for nth-child
                is_last_child: node_hierarchy[child_id].next_sibling.is_none(),
            };

            nodes[child_id.index()] = child_html_matcher;
        }
    }

    NodeDataContainer { internal: nodes }
}

/// TODO: This is wrong, but it's fast
pub fn classify_css_path(path: &CssPath) -> StyledNodeState {
    use azul_css::{CssPathSelector::*, CssPathPseudoSelector::*};
    match path.selectors.as_ref().last() {
        Some(PseudoSelector(Hover)) => StyledNodeState::Hover,
        Some(PseudoSelector(Active)) => StyledNodeState::Active,
        Some(PseudoSelector(Focus)) => StyledNodeState::Focused,
        _ => StyledNodeState::Normal
    }
}

/// Matches a single group of items, panics on Children or DirectChildren selectors
///
/// The intent is to "split" the CSS path into groups by selectors, then store and cache
/// whether the direct or any parent has matched the path correctly
pub(crate) fn selector_group_matches(
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
                return false;
                // if !html_node.is_hovered_over { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Active) => {
                return false;
                // if !html_node.is_active { return false; }
            },
            PseudoSelector(CssPathPseudoSelector::Focus) => {
                return false;
                // if !html_node.is_focused { return false; }
            },
            DirectChildren | Children => {
                panic!("Unreachable: DirectChildren or Children in CSS path!");
            },
        }
    }

    true
}

/// Applies the property to the element, returns the previous `Option<CssProperty>` if it has changed
pub(crate) fn apply_style_property(style: &mut RectStyle, layout: &mut RectLayout, property: &CssProperty) -> Option<CssProperty> {

    use azul_css::CssProperty::*;
    use azul_css::CssPropertyType;

    match property {

        Display(d)                      => { let previous_property = layout.display.clone(); layout.display = Some(*d); if previous_property != layout.display { Some(match previous_property { Some(s) => Display(s), None => CssProperty::none(CssPropertyType::Display) }) } else { None } },
        Float(f)                        => { let previous_property = layout.float.clone(); layout.float = Some(*f); if previous_property != layout.float { Some(match previous_property { Some(s) => Float(s), None => CssProperty::none(CssPropertyType::Float) }) } else { None } },
        BoxSizing(bs)                   => { let previous_property = layout.box_sizing.clone(); layout.box_sizing = Some(*bs); if previous_property != layout.box_sizing { Some(match previous_property { Some(s) => BoxSizing(s), None => CssProperty::none(CssPropertyType::BoxSizing) }) } else { None } },
        Width(w)                        => { let previous_property = layout.width.clone(); layout.width = Some(*w); if previous_property != layout.width { Some(match previous_property { Some(s) => Width(s), None => CssProperty::none(CssPropertyType::Width) }) } else { None } },
        Height(h)                       => { let previous_property = layout.height.clone(); layout.height = Some(*h); if previous_property != layout.height { Some(match previous_property { Some(s) => Height(s), None => CssProperty::none(CssPropertyType::Height) }) } else { None } },
        MinWidth(mw)                    => { let previous_property = layout.min_width.clone(); layout.min_width = Some(*mw); if previous_property != layout.min_width { Some(match previous_property { Some(s) => MinWidth(s), None => CssProperty::none(CssPropertyType::MinWidth) }) } else { None } },
        MinHeight(mh)                   => { let previous_property = layout.min_height.clone(); layout.min_height = Some(*mh); if previous_property != layout.min_height { Some(match previous_property { Some(s) => MinHeight(s), None => CssProperty::none(CssPropertyType::MinHeight) }) } else { None } },
        MaxWidth(mw)                    => { let previous_property = layout.max_width.clone(); layout.max_width = Some(*mw); if previous_property != layout.max_width { Some(match previous_property { Some(s) => MaxWidth(s), None => CssProperty::none(CssPropertyType::MaxWidth) }) } else { None } },
        MaxHeight(mh)                   => { let previous_property = layout.max_height.clone(); layout.max_height = Some(*mh); if previous_property != layout.max_height { Some(match previous_property { Some(s) => MaxHeight(s), None => CssProperty::none(CssPropertyType::MaxHeight) }) } else { None } },
        Position(p)                     => { let previous_property = layout.position.clone(); layout.position = Some(*p); if previous_property != layout.position { Some(match previous_property { Some(s) => Position(s), None => CssProperty::none(CssPropertyType::Position) }) } else { None } },
        Top(t)                          => { let previous_property = layout.top.clone(); layout.top = Some(*t); if previous_property != layout.top { Some(match previous_property { Some(s) => Top(s), None => CssProperty::none(CssPropertyType::Top) }) } else { None } },
        Bottom(b)                       => { let previous_property = layout.bottom.clone(); layout.bottom = Some(*b); if previous_property != layout.bottom { Some(match previous_property { Some(s) => Bottom(s), None => CssProperty::none(CssPropertyType::Bottom) }) } else { None } },
        Right(r)                        => { let previous_property = layout.right.clone(); layout.right = Some(*r); if previous_property != layout.right { Some(match previous_property { Some(s) => Right(s), None => CssProperty::none(CssPropertyType::Right) }) } else { None } },
        Left(l)                         => { let previous_property = layout.left.clone(); layout.left = Some(*l); if previous_property != layout.left { Some(match previous_property { Some(s) => Left(s), None => CssProperty::none(CssPropertyType::Left) }) } else { None } },
        FlexWrap(fw)                    => { let previous_property = layout.wrap.clone(); layout.wrap = Some(*fw); if previous_property != layout.wrap { Some(match previous_property { Some(s) => FlexWrap(s), None => CssProperty::none(CssPropertyType::FlexWrap) }) } else { None } },
        FlexDirection(fd)               => { let previous_property = layout.direction.clone(); layout.direction = Some(*fd); if previous_property != layout.direction { Some(match previous_property { Some(s) => FlexDirection(s), None => CssProperty::none(CssPropertyType::FlexDirection) }) } else { None } },
        FlexGrow(fg)                    => { let previous_property = layout.flex_grow.clone(); layout.flex_grow = Some(*fg); if previous_property != layout.flex_grow { Some(match previous_property { Some(s) => FlexGrow(s), None => CssProperty::none(CssPropertyType::FlexGrow) }) } else { None } },
        FlexShrink(fs)                  => { let previous_property = layout.flex_shrink.clone(); layout.flex_shrink = Some(*fs); if previous_property != layout.flex_shrink { Some(match previous_property { Some(s) => FlexShrink(s), None => CssProperty::none(CssPropertyType::FlexShrink) }) } else { None } },
        JustifyContent(jc)              => { let previous_property = layout.justify_content.clone(); layout.justify_content = Some(*jc); if previous_property != layout.justify_content { Some(match previous_property { Some(s) => JustifyContent(s), None => CssProperty::none(CssPropertyType::JustifyContent) }) } else { None } },
        AlignItems(ai)                  => { let previous_property = layout.align_items.clone(); layout.align_items = Some(*ai); if previous_property != layout.align_items { Some(match previous_property { Some(s) => AlignItems(s), None => CssProperty::none(CssPropertyType::AlignItems) }) } else { None } },
        AlignContent(ac)                => { let previous_property = layout.align_content.clone(); layout.align_content = Some(*ac); if previous_property != layout.align_content { Some(match previous_property { Some(s) => AlignContent(s), None => CssProperty::none(CssPropertyType::AlignContent) }) } else { None } },
        OverflowX(ox)                   => { let previous_property = layout.overflow_x.clone(); layout.overflow_x = Some(*ox); if previous_property != layout.overflow_x { Some(match previous_property { Some(s) => OverflowX(s), None => CssProperty::none(CssPropertyType::OverflowX) }) } else { None } },
        OverflowY(oy)                   => { let previous_property = layout.overflow_y.clone(); layout.overflow_y = Some(*oy); if previous_property != layout.overflow_y { Some(match previous_property { Some(s) => OverflowY(s), None => CssProperty::none(CssPropertyType::OverflowY) }) } else { None } },
        PaddingTop(pt)                  => { let previous_property = layout.padding_top.clone(); layout.padding_top = Some(*pt); if previous_property != layout.padding_top { Some(match previous_property { Some(s) => PaddingTop(s), None => CssProperty::none(CssPropertyType::PaddingTop) }) } else { None } },
        PaddingLeft(pl)                 => { let previous_property = layout.padding_left.clone(); layout.padding_left = Some(*pl); if previous_property != layout.padding_left { Some(match previous_property { Some(s) => PaddingLeft(s), None => CssProperty::none(CssPropertyType::PaddingLeft) }) } else { None } },
        PaddingRight(pr)                => { let previous_property = layout.padding_right.clone(); layout.padding_right = Some(*pr); if previous_property != layout.padding_right { Some(match previous_property { Some(s) => PaddingRight(s), None => CssProperty::none(CssPropertyType::PaddingRight) }) } else { None } },
        PaddingBottom(pb)               => { let previous_property = layout.padding_bottom.clone(); layout.padding_bottom = Some(*pb); if previous_property != layout.padding_bottom { Some(match previous_property { Some(s) => PaddingBottom(s), None => CssProperty::none(CssPropertyType::PaddingBottom) }) } else { None } },
        MarginTop(mt)                   => { let previous_property = layout.margin_top.clone(); layout.margin_top = Some(*mt); if previous_property != layout.margin_top { Some(match previous_property { Some(s) => MarginTop(s), None => CssProperty::none(CssPropertyType::MarginTop) }) } else { None } },
        MarginLeft(ml)                  => { let previous_property = layout.margin_left.clone(); layout.margin_left = Some(*ml); if previous_property != layout.margin_left { Some(match previous_property { Some(s) => MarginLeft(s), None => CssProperty::none(CssPropertyType::MarginLeft) }) } else { None } },
        MarginRight(mr)                 => { let previous_property = layout.margin_right.clone(); layout.margin_right = Some(*mr); if previous_property != layout.margin_right { Some(match previous_property { Some(s) => MarginRight(s), None => CssProperty::none(CssPropertyType::MarginRight) }) } else { None } },
        MarginBottom(mb)                => { let previous_property = layout.margin_bottom.clone(); layout.margin_bottom = Some(*mb); if previous_property != layout.margin_bottom { Some(match previous_property { Some(s) => MarginBottom(s), None => CssProperty::none(CssPropertyType::MarginBottom) }) } else { None } },
        BorderTopWidth(btw)             => { let previous_property = layout.border_top_width.clone(); layout.border_top_width = Some(*btw); if previous_property != layout.border_top_width { Some(match previous_property { Some(s) => BorderTopWidth(s), None => CssProperty::none(CssPropertyType::BorderTopWidth) }) } else { None } },
        BorderRightWidth(brw)           => { let previous_property = layout.border_right_width.clone(); layout.border_right_width = Some(*brw); if previous_property != layout.border_right_width { Some(match previous_property { Some(s) => BorderRightWidth(s), None => CssProperty::none(CssPropertyType::BorderRightWidth) }) } else { None } },
        BorderLeftWidth(blw)            => { let previous_property = layout.border_left_width.clone(); layout.border_left_width = Some(*blw); if previous_property != layout.border_left_width { Some(match previous_property { Some(s) => BorderLeftWidth(s), None => CssProperty::none(CssPropertyType::BorderLeftWidth) }) } else { None } },
        BorderBottomWidth(bbw)          => { let previous_property = layout.border_bottom_width.clone(); layout.border_bottom_width = Some(*bbw); if previous_property != layout.border_bottom_width { Some(match previous_property { Some(s) => BorderBottomWidth(s), None => CssProperty::none(CssPropertyType::BorderBottomWidth) }) } else { None } },

        TextColor(c)                    => { let previous_property = style.text_color.clone(); style.text_color = Some(*c); if previous_property != style.text_color { Some(match previous_property { Some(s) => TextColor(s), None => CssProperty::none(CssPropertyType::TextColor)}) } else { None } },
        FontSize(fs)                    => { let previous_property = style.font_size.clone(); style.font_size = Some(*fs); if previous_property != style.font_size { Some(match previous_property { Some(s) => FontSize(s), None => CssProperty::none(CssPropertyType::FontSize)}) } else { None } },
        FontFamily(ff)                  => { let previous_property = style.font_family.clone(); style.font_family = Some(ff.clone()); if previous_property != style.font_family { Some(match previous_property { Some(s) => FontFamily(s), None => CssProperty::none(CssPropertyType::FontFamily)}) } else { None } },
        TextAlign(ta)                   => { let previous_property = style.text_align.clone(); style.text_align = Some(*ta); if previous_property != style.text_align { Some(match previous_property { Some(s) => TextAlign(s), None => CssProperty::none(CssPropertyType::TextAlign)}) } else { None } },
        LetterSpacing(ls)               => { let previous_property = style.letter_spacing.clone(); style.letter_spacing = Some(*ls); if previous_property != style.letter_spacing { Some(match previous_property { Some(s) => LetterSpacing(s), None => CssProperty::none(CssPropertyType::LetterSpacing)}) } else { None } },
        LineHeight(lh)                  => { let previous_property = style.line_height.clone(); style.line_height = Some(*lh); if previous_property != style.line_height { Some(match previous_property { Some(s) => LineHeight(s), None => CssProperty::none(CssPropertyType::LineHeight)}) } else { None } },
        WordSpacing(ws)                 => { let previous_property = style.word_spacing.clone(); style.word_spacing = Some(*ws); if previous_property != style.word_spacing { Some(match previous_property { Some(s) => WordSpacing(s), None => CssProperty::none(CssPropertyType::WordSpacing)}) } else { None } },
        TabWidth(tw)                    => { let previous_property = style.tab_width.clone(); style.tab_width = Some(*tw); if previous_property != style.tab_width { Some(match previous_property { Some(s) => TabWidth(s), None => CssProperty::none(CssPropertyType::TabWidth)}) } else { None } },
        Cursor(c)                       => { let previous_property = style.cursor.clone(); style.cursor = Some(*c); if previous_property != style.cursor { Some(match previous_property { Some(s) => Cursor(s), None => CssProperty::none(CssPropertyType::Cursor)}) } else { None } },
        BackgroundContent(bc)           => { let previous_property = style.background.clone(); style.background = Some(bc.clone()); if previous_property != style.background { Some(match previous_property { Some(s) => BackgroundContent(s), None => CssProperty::none(CssProperty::background_content(bc.clone().get_property_or_default().unwrap_or_default()).get_type())}) } else { None } },
        BackgroundPosition(bp)          => { let previous_property = style.background_position.clone(); style.background_position = Some(*bp); if previous_property != style.background_position { Some(match previous_property { Some(s) => BackgroundPosition(s), None => CssProperty::none(CssPropertyType::BackgroundPosition)}) } else { None } },
        BackgroundSize(bs)              => { let previous_property = style.background_size.clone(); style.background_size = Some(*bs); if previous_property != style.background_size { Some(match previous_property { Some(s) => BackgroundSize(s), None => CssProperty::none(CssPropertyType::BackgroundSize)}) } else { None } },
        BackgroundRepeat(br)            => { let previous_property = style.background_repeat.clone(); style.background_repeat = Some(*br); if previous_property != style.background_repeat { Some(match previous_property { Some(s) => BackgroundRepeat(s), None => CssProperty::none(CssPropertyType::BackgroundRepeat)}) } else { None } },
        BorderTopLeftRadius(btl)        => { let previous_property = style.border_top_left_radius.clone(); style.border_top_left_radius = Some(*btl); if previous_property != style.border_top_left_radius { Some(match previous_property { Some(s) => BorderTopLeftRadius(s), None => CssProperty::none(CssPropertyType::BorderTopLeftRadius)}) } else { None } },
        BorderTopRightRadius(btr)       => { let previous_property = style.border_top_right_radius.clone(); style.border_top_right_radius = Some(*btr); if previous_property != style.border_top_right_radius { Some(match previous_property { Some(s) => BorderTopRightRadius(s), None => CssProperty::none(CssPropertyType::BorderTopRightRadius)}) } else { None } },
        BorderBottomLeftRadius(bbl)     => { let previous_property = style.border_bottom_left_radius.clone(); style.border_bottom_left_radius = Some(*bbl); if previous_property != style.border_bottom_left_radius { Some(match previous_property { Some(s) => BorderBottomLeftRadius(s), None => CssProperty::none(CssPropertyType::BorderBottomLeftRadius)}) } else { None } },
        BorderBottomRightRadius(bbr)    => { let previous_property = style.border_bottom_right_radius.clone(); style.border_bottom_right_radius = Some(*bbr); if previous_property != style.border_bottom_right_radius { Some(match previous_property { Some(s) => BorderBottomRightRadius(s), None => CssProperty::none(CssPropertyType::BorderBottomRightRadius)}) } else { None } },
        BorderTopColor(btc)             => { let previous_property = style.border_top_color.clone(); style.border_top_color = Some(*btc); if previous_property != style.border_top_color { Some(match previous_property { Some(s) => BorderTopColor(s), None => CssProperty::none(CssPropertyType::BorderTopColor)}) } else { None } },
        BorderRightColor(brc)           => { let previous_property = style.border_right_color.clone(); style.border_right_color = Some(*brc); if previous_property != style.border_right_color { Some(match previous_property { Some(s) => BorderRightColor(s), None => CssProperty::none(CssPropertyType::BorderRightColor)}) } else { None } },
        BorderLeftColor(blc)            => { let previous_property = style.border_left_color.clone(); style.border_left_color = Some(*blc); if previous_property != style.border_left_color { Some(match previous_property { Some(s) => BorderLeftColor(s), None => CssProperty::none(CssPropertyType::BorderLeftColor)}) } else { None } },
        BorderBottomColor(bbc)          => { let previous_property = style.border_bottom_color.clone(); style.border_bottom_color = Some(*bbc); if previous_property != style.border_bottom_color { Some(match previous_property { Some(s) => BorderBottomColor(s), None => CssProperty::none(CssPropertyType::BorderBottomColor)}) } else { None } },
        BorderTopStyle(bts)             => { let previous_property = style.border_top_style.clone(); style.border_top_style = Some(*bts); if previous_property != style.border_top_style { Some(match previous_property { Some(s) => BorderTopStyle(s), None => CssProperty::none(CssPropertyType::BorderTopStyle)}) } else { None } },
        BorderRightStyle(brs)           => { let previous_property = style.border_right_style.clone(); style.border_right_style = Some(*brs); if previous_property != style.border_right_style { Some(match previous_property { Some(s) => BorderRightStyle(s), None => CssProperty::none(CssPropertyType::BorderRightStyle)}) } else { None } },
        BorderLeftStyle(bls)            => { let previous_property = style.border_left_style.clone(); style.border_left_style = Some(*bls); if previous_property != style.border_left_style { Some(match previous_property { Some(s) => BorderLeftStyle(s), None => CssProperty::none(CssPropertyType::BorderLeftStyle)}) } else { None } },
        BorderBottomStyle(bbs)          => { let previous_property = style.border_bottom_style.clone(); style.border_bottom_style = Some(*bbs); if previous_property != style.border_bottom_style { Some(match previous_property { Some(s) => BorderBottomStyle(s), None => CssProperty::none(CssPropertyType::BorderBottomStyle)}) } else { None } },
        BoxShadowLeft(bsl)              => { let previous_property = style.box_shadow_left.clone(); style.box_shadow_left = Some(*bsl); if previous_property != style.box_shadow_left { Some(match previous_property { Some(s) => BoxShadowLeft(s), None => CssProperty::none(CssPropertyType::BoxShadowLeft)}) } else { None } },
        BoxShadowRight(bsr)             => { let previous_property = style.box_shadow_right.clone(); style.box_shadow_right = Some(*bsr); if previous_property != style.box_shadow_right { Some(match previous_property { Some(s) => BoxShadowRight(s), None => CssProperty::none(CssPropertyType::BoxShadowRight)}) } else { None } },
        BoxShadowTop(bst)               => { let previous_property = style.box_shadow_top.clone(); style.box_shadow_top = Some(*bst); if previous_property != style.box_shadow_top { Some(match previous_property { Some(s) => BoxShadowTop(s), None => CssProperty::none(CssPropertyType::BoxShadowTop)}) } else { None } },
        BoxShadowBottom(bsb)            => { let previous_property = style.box_shadow_bottom.clone(); style.box_shadow_bottom = Some(*bsb); if previous_property != style.box_shadow_bottom { Some(match previous_property { Some(s) => BoxShadowBottom(s), None => CssProperty::none(CssPropertyType::BoxShadowBottom)}) } else { None } },
        Opacity(so)                     => { let previous_property = style.opacity.clone(); style.opacity = Some(*so); if previous_property != style.opacity { Some(match previous_property { Some(s) => Opacity(s), None => CssProperty::none(CssPropertyType::Opacity)}) } else { None } }
        Transform(t)                    => { let previous_property = style.transform.clone(); style.transform = Some(t.clone()); if previous_property != style.transform { Some(match previous_property { Some(s) => Transform(s), None => CssProperty::none(CssPropertyType::Transform)}) } else { None } }
        TransformOrigin(to)             => { let previous_property = style.transform_origin.clone(); style.transform_origin = Some(*to); if previous_property != style.transform_origin { Some(match previous_property { Some(s) => TransformOrigin(s), None => CssProperty::none(CssPropertyType::TransformOrigin)}) } else { None } }
        PerspectiveOrigin(po)           => { let previous_property = style.perspective_origin.clone(); style.perspective_origin = Some(*po); if previous_property != style.perspective_origin { Some(match previous_property { Some(s) => PerspectiveOrigin(s), None => CssProperty::none(CssPropertyType::PerspectiveOrigin)}) } else { None } }
        BackfaceVisibility(bfv)         => { let previous_property = style.backface_visibility.clone(); style.backface_visibility = Some(*bfv); if previous_property != style.backface_visibility { Some(match previous_property { Some(s) => BackfaceVisibility(s), None => CssProperty::none(CssPropertyType::BackfaceVisibility)}) } else { None } }
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