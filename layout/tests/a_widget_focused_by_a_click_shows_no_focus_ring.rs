//! No widget shows a focus ring when a click focused it.
//!
//! A browser rings a control only when its focus should be seen
//! (`:focus-visible`: reached with Tab), and a click focuses without a ring.
//! The buttons and the switch ring that way; the theme helpers every other
//! widget rings with (`decl::focus_ring`, `focus_halo`, `focus_halo_inset`
//! and their stacked forms) declared their rings under `:focus`, which a
//! click raises as well - the office shell's panes, the tabs, the chips,
//! the segments, the pages of a pagination all lit up under the pointer.
//!
//! Every focusable node of each widget, in both themes, is focused the way a
//! click focuses it (the plain focus restyle); its ring slots must read what
//! they read at rest.

use azul_core::{
    callbacks::Update,
    dom::{Dom, NodeId},
    refany::RefAny,
    styled_dom::{FocusChange, StyledDom},
};
use azul_css::props::property::{CssProperty, CssPropertyType};
use azul_layout::{
    callbacks::CallbackInfo,
    widgets::{
        chip::{Chip, ChipState},
        pagination::{Pagination, PaginationState},
        segmented::{Segmented, SegmentedState},
        shells::office_shell::{OfficeShell, ShellPane},
        tabs::{TabHeader, TabHeaderState},
        themes::UiTheme,
    },
};

/// The properties a theme draws a ring with: the four shadow slots and the
/// four border colours.
const RING_SLOTS: [CssPropertyType; 8] = [
    CssPropertyType::BoxShadowLeft,
    CssPropertyType::BoxShadowRight,
    CssPropertyType::BoxShadowTop,
    CssPropertyType::BoxShadowBottom,
    CssPropertyType::BorderTopColor,
    CssPropertyType::BorderRightColor,
    CssPropertyType::BorderBottomColor,
    CssPropertyType::BorderLeftColor,
];

fn ring_slots(sd: &StyledDom, node: NodeId) -> Vec<Option<CssProperty>> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    RING_SLOTS
        .iter()
        .map(|ty| cache.get_property(node_data, &node, state, ty).cloned())
        .collect()
}

/// Focuses every focusable node of `control` as a click does and checks
/// that none of them changes a ring slot.
fn assert_no_part_rings_on_a_click(what: &str, control: Dom) {
    let mut dom = Dom::create_body().with_child(control);
    let mut sd = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let focusable: Vec<NodeId> = {
        let nodes = sd.node_data.as_container();
        (0..nodes.len())
            .map(NodeId::new)
            .filter(|n| nodes[*n].get_tab_index().is_some())
            .collect()
    };
    assert!(!focusable.is_empty(), "harness: {what} has a focusable part");
    for node in focusable {
        let at_rest = ring_slots(&sd, node);
        let _ = sd.restyle_on_state_change(
            Some(FocusChange {
                lost_focus: None,
                gained_focus: Some(node),
            }),
            None,
            None,
        );
        assert_eq!(
            ring_slots(&sd, node),
            at_rest,
            "{what}: a click focused node {node:?} and a ring appeared"
        );
        let _ = sd.restyle_on_state_change(
            Some(FocusChange {
                lost_focus: Some(node),
                gained_focus: None,
            }),
            None,
            None,
        );
    }
}

extern "C" fn on_tab(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
    Update::DoNothing
}
extern "C" fn on_chip(_: RefAny, _: CallbackInfo, _: ChipState) -> Update {
    Update::DoNothing
}
extern "C" fn on_segment(_: RefAny, _: CallbackInfo, _: SegmentedState) -> Update {
    Update::DoNothing
}
extern "C" fn on_page(_: RefAny, _: CallbackInfo, _: PaginationState) -> Update {
    Update::DoNothing
}

fn labels(items: &[&str]) -> azul_css::StringVec {
    azul_css::StringVec::from_vec(items.iter().map(|s| azul_css::AzString::from(*s)).collect())
}

#[test]
fn a_widget_focused_by_a_click_shows_no_focus_ring() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        let name = |w: &str| format!("{w} ({theme:?})");
        assert_no_part_rings_on_a_click(
            &name("the office shell's panes"),
            OfficeShell::create()
                .with_pane(ShellPane::create("doc".into(), Dom::create_div()))
                .with_theme(theme)
                .dom(),
        );
        assert_no_part_rings_on_a_click(
            &name("a tab bar"),
            TabHeader::create(labels(&["One", "Two", "Three"]))
                .with_active_tab(1)
                .with_on_click(RefAny::new(()), on_tab as azul_layout::widgets::tabs::TabOnClickCallbackType)
                .with_theme(theme)
                .dom(),
        );
        assert_no_part_rings_on_a_click(
            &name("a chip"),
            Chip::create("Tag".into())
                .with_on_click(RefAny::new(()), on_chip as azul_layout::widgets::chip::ChipOnClickCallbackType)
                .with_theme(theme)
                .dom(),
        );
        assert_no_part_rings_on_a_click(
            &name("a segmented control"),
            Segmented::create(labels(&["Day", "Week", "Month"]))
                .with_on_change(
                    RefAny::new(()),
                    on_segment as azul_layout::widgets::segmented::SegmentedOnChangeCallbackType,
                )
                .with_theme(theme)
                .dom(),
        );
        assert_no_part_rings_on_a_click(
            &name("a pagination"),
            Pagination::create(2, 5)
                .with_on_change(
                    RefAny::new(()),
                    on_page as azul_layout::widgets::pagination::PaginationOnChangeCallbackType,
                )
                .with_theme(theme)
                .dom(),
        );
    }
}
