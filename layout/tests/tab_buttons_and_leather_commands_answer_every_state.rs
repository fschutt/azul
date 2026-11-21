//! Tab buttons and the ribbon's commands on the leather answer every state
//! as the design draws them:
//!
//! - hovered, a plain tab button's face lifts to the leather's hover face
//!   (#FDFCF9 at 90% on top) and its gold edge brightens (#E8CF8C on top);
//!   the selected tab (the stone) brightens its gold the same way;
//! - pressed, the plain face sinks to the leather's pressed face (#E2DFD7
//!   at 90% on top);
//! - focused from the keyboard, a tab button rings (a `:focus-visible`
//!   shadow);
//! - a ribbon command hovered brightens its gold; a toggled one rests
//!   recessed in the accent's frost (#E2E6F0 at 92% on top) and keeps it;
//! - a check box's row (the box and its label) is a frosted recessed panel
//!   (#FBFAF7 at 90% on top).

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::{ActiveChange, HoverChange, StyledDom},
};
use azul_css::{
    dynamic_selector::{DynamicSelector, PseudoStateType},
    props::{
        basic::color::{ColorOrSystem, ColorU},
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
    AzString, StringVec,
};
use azul_layout::widgets::{
    check_box::CheckBox,
    ribbon::{
        Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonRow, RibbonTab, RibbonTabVec,
    },
    tabs::{TabHeader, TabStyle},
    themes::UiTheme,
};

const ACTIVE: &str = "__azul-native-tabs-tab-active";
const NOT_ACTIVE: &str = "__azul-native-tabs-tab-not-active";

fn classed(sd: &StyledDom, class: &str) -> Vec<NodeId> {
    let nodes = sd.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .filter(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .collect()
}

/// `node` and every ancestor: the chain a pointer enters or presses.
fn chain(sd: &StyledDom, node: NodeId) -> Vec<NodeId> {
    let hierarchy = sd.node_hierarchy.as_container();
    let mut out = vec![node];
    let mut cur = hierarchy[node].parent_id();
    while let Some(n) = cur {
        out.push(n);
        cur = hierarchy[n].parent_id();
    }
    out
}

fn value(sd: &StyledDom, node: NodeId, ty: CssPropertyType) -> Option<CssProperty> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    cache.get_property(node_data, &node, state, &ty).cloned()
}

fn top_edge(sd: &StyledDom, node: NodeId) -> Option<ColorU> {
    match value(sd, node, CssPropertyType::BorderTopColor) {
        Some(CssProperty::BorderTopColor(v)) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

/// The top colour of the face: the first stop of the last layer that is a
/// gradient.
fn face_top(sd: &StyledDom, node: NodeId) -> Option<ColorU> {
    match value(sd, node, CssPropertyType::BackgroundContent) {
        Some(CssProperty::BackgroundContent(v)) => v.get_property().and_then(|layers| {
            layers.as_ref().iter().rev().find_map(|l| match l {
                StyleBackgroundContent::LinearGradient(g) => match g.stops.as_ref().first()?.color {
                    ColorOrSystem::Color(c) => Some(c),
                    ColorOrSystem::System(_) => None,
                },
                _ => None,
            })
        }),
        _ => None,
    }
}

fn row() -> StyledDom {
    let labels = StringVec::from_vec(
        ["File", "Home", "Share"]
            .iter()
            .map(|s| AzString::from(*s))
            .collect(),
    );
    let mut dom = Dom::create_body().with_child(
        TabHeader::create(labels)
            .with_active_tab(1)
            .with_theme(UiTheme::Flora)
            .with_style(TabStyle::Pills)
            .dom(),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn hover(sd: &mut StyledDom, node: NodeId) {
    let entered = chain(sd, node);
    let _ = sd.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: entered,
        }),
        None,
    );
}

fn press(sd: &mut StyledDom, node: NodeId) {
    let activated = chain(sd, node);
    let _ = sd.restyle_on_state_change(
        None,
        None,
        Some(ActiveChange {
            deactivated: Vec::new(),
            activated,
        }),
    );
}

const GOLD_HOVER_TOP: ColorU = ColorU::rgb(0xE8, 0xCF, 0x8C);

#[test]
fn a_hovered_tab_button_lifts_its_face_and_brightens_its_gold() {
    let mut sd = row();
    let plain = classed(&sd, NOT_ACTIVE)[0];
    assert_eq!(top_edge(&sd, plain), Some(ColorU::rgb(0xDC, 0xC1, 0x7C)), "premise: the gold edge");
    hover(&mut sd, plain);
    assert_eq!(top_edge(&sd, plain), Some(GOLD_HOVER_TOP), "the gold brightens");
    assert_eq!(
        face_top(&sd, plain),
        Some(ColorU::new(253, 252, 249, 230)),
        "the leather's hover face"
    );
    let mut sd = row();
    let stone = classed(&sd, ACTIVE)[0];
    hover(&mut sd, stone);
    assert_eq!(top_edge(&sd, stone), Some(GOLD_HOVER_TOP), "the stone's gold brightens too");
}

#[test]
fn a_pressed_tab_button_sinks_to_the_leather_s_pressed_face() {
    let mut sd = row();
    let plain = classed(&sd, NOT_ACTIVE)[0];
    press(&mut sd, plain);
    assert_eq!(face_top(&sd, plain), Some(ColorU::new(226, 223, 215, 230)));
}

#[test]
fn a_tab_button_rings_when_the_keyboard_focuses_it() {
    let sd = row();
    for node in [classed(&sd, NOT_ACTIVE)[0], classed(&sd, ACTIVE)[0]] {
        let rings = sd.node_data.as_container()[node]
            .style
            .iter_inline_properties()
            .any(|(p, conds)| {
                matches!(p, CssProperty::BoxShadowLeft(_))
                    && conds.as_ref().iter().any(|c| {
                        matches!(c, DynamicSelector::PseudoState(PseudoStateType::FocusVisible))
                    })
            });
        assert!(rings, "a focus-visible ring on {node:?}");
    }
}

fn ribbon() -> StyledDom {
    let group = RibbonGroup::new("Panes".into()).with_items(
        vec![
            RibbonItem::LargeButton(RibbonButton::new("view_sidebar".into(), "Navigation pane".into())),
            RibbonItem::LargeButton(
                RibbonButton::new("preview".into(), "Preview pane".into()).with_toggled(true),
            ),
        ]
        .into(),
    );
    let mut dom = Dom::create_body().with_child(
        Ribbon::new(RibbonTabVec::from_vec(vec![RibbonTab::new("VIEW".into()).with_group(group)]))
            .with_theme(UiTheme::Flora)
            .with_tab_style(TabStyle::Pills)
            .with_available_width(1280.0)
            .dom_desktop(),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

#[test]
fn a_command_on_the_leather_brightens_its_gold_and_a_toggled_one_rests_in_the_frost() {
    let mut sd = ribbon();
    let buttons = classed(&sd, "__azul-native-button");
    assert_eq!(buttons.len(), 2, "the two commands");
    let (plain, toggled) = (buttons[0], buttons[1]);
    assert_eq!(
        face_top(&sd, toggled),
        Some(ColorU::new(226, 230, 240, 235)),
        "the toggled command is recessed in the accent's frost"
    );
    assert_eq!(
        top_edge(&sd, toggled),
        Some(ColorU::rgb(0xDC, 0xC1, 0x7C)),
        "and keeps its gold"
    );
    hover(&mut sd, plain);
    assert_eq!(top_edge(&sd, plain), Some(GOLD_HOVER_TOP), "the gold brightens under the pointer");
}

#[test]
fn a_check_box_s_row_on_the_leather_is_a_frosted_panel() {
    let row = RibbonRow::default()
        .with_item(RibbonItem::Check(CheckBox::create(true)))
        .with_item(RibbonItem::Custom(Dom::create_span_with_text("Item check boxes")));
    let group = RibbonGroup::new("Show/hide".into())
        .with_items(vec![RibbonItem::Column(
            azul_layout::widgets::ribbon::RibbonColumn::default().with_item(RibbonItem::Row(row)),
        )]
        .into());
    let mut dom = Dom::create_body().with_child(
        Ribbon::new(RibbonTabVec::from_vec(vec![RibbonTab::new("VIEW".into()).with_group(group)]))
            .with_theme(UiTheme::Flora)
            .with_tab_style(TabStyle::Pills)
            .with_available_width(1280.0)
            .dom_desktop(),
    );
    let sd = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let rows = classed(&sd, "__azul-native-ribbon-row");
    assert_eq!(rows.len(), 1, "the check box's row");
    assert_eq!(face_top(&sd, rows[0]), Some(ColorU::new(251, 250, 247, 230)), "frosted");
}
