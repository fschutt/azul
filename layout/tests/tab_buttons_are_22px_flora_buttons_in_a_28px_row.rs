//! Tab buttons (`TabStyle::Pills`, "Buttons" in the UI) are flora's own
//! commands in a 28px row, the height of a title bar:
//!
//! - every button is 22px tall and centred in the row (14px from its top,
//!   where the traffic lights are centred when the row is the title bar);
//! - the buttons stand 6px apart, the first 10px in from the strip's edge;
//! - the row has no padding above or below the buttons, and a 2px golden
//!   line closes it, under the row;
//! - a button has the house radius (3px, not a pill's), its label in the
//!   capitals: 11.5px, bold, tracked .08em, the trailing tracking taken back.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{props::property::CssProperty, AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        tabs::{TabHeader, TabStyle},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ACTIVE: &str = "__azul-native-tabs-tab-active";
const NOT_ACTIVE: &str = "__azul-native-tabs-tab-not-active";
const HEADER: &str = "__azul-native-tabs-header";
const RULE: &str = "__azul-native-tabs-rule";

fn labels() -> StringVec {
    StringVec::from_vec(
        ["File", "Home", "Share", "View"]
            .iter()
            .map(|l| AzString::from(*l))
            .collect(),
    )
}

fn pills(active: usize) -> Dom {
    TabHeader::create(labels())
        .with_active_tab(active)
        .with_theme(UiTheme::Flora)
        .with_style(TabStyle::Pills)
        .dom()
}

fn laid_out(dom: Dom) -> LayoutWindow {
    let body = Dom::create_body().with_css("margin: 0px;").with_child(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(body),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the row lays out");
    lw
}

fn rects_of(lw: &LayoutWindow, class: &str) -> Vec<LogicalRect> {
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let nodes = result.styled_dom.node_data.as_container();
    let mut rects: Vec<LogicalRect> = (0..nodes.len())
        .map(NodeId::new)
        .filter(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .filter_map(|n| {
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(n)),
            })
        })
        .collect();
    rects.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    rects
}

fn buttons(lw: &LayoutWindow) -> Vec<LogicalRect> {
    let mut v = rects_of(lw, ACTIVE);
    v.extend(rects_of(lw, NOT_ACTIVE));
    v.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    v
}

#[test]
fn every_tab_button_is_22px_tall_and_centred_in_the_28px_row() {
    for active in [1, usize::MAX] {
        let lw = laid_out(pills(active));
        let header = rects_of(&lw, HEADER)[0];
        let buttons = buttons(&lw);
        assert_eq!(buttons.len(), 4, "four buttons");
        for b in &buttons {
            assert!((b.size.height - 22.0).abs() < 0.01, "a 22px button: {b:?}");
            let centre = b.origin.y + b.size.height / 2.0 - header.origin.y;
            assert!((centre - 14.0).abs() < 0.5, "centred 14px from the row's top: {b:?}");
        }
    }
}

#[test]
fn the_tab_buttons_stand_6px_apart_from_10px_in() {
    let lw = laid_out(pills(1));
    let header = rects_of(&lw, HEADER)[0];
    let buttons = buttons(&lw);
    assert!(
        (buttons[0].origin.x - header.origin.x - 10.0).abs() < 0.5,
        "the first button 10px in: {:?}",
        buttons[0]
    );
    for w in buttons.windows(2) {
        let gap = w[1].origin.x - (w[0].origin.x + w[0].size.width);
        assert!((gap - 6.0).abs() < 0.5, "6px between two buttons, not {gap}");
    }
}

#[test]
fn a_2px_golden_line_closes_the_row_under_it() {
    let lw = laid_out(pills(1));
    let header = rects_of(&lw, HEADER)[0];
    let rule = rects_of(&lw, RULE);
    assert_eq!(rule.len(), 1, "one line");
    let rule = rule[0];
    assert!((rule.size.height - 2.0).abs() < 0.01, "2px: {rule:?}");
    assert!(
        (rule.origin.y - header.origin.y - 28.0).abs() < 0.5,
        "under the 28px row: {rule:?}"
    );
    assert!(
        (header.size.height - 30.0).abs() < 0.5,
        "the strip is the row and its line, nothing else: {header:?}"
    );
    assert!((rule.size.width - header.size.width).abs() < 0.5, "the whole width");
}

/// The last unconditioned declaration `pick` reads on `dom`'s own node.
fn declared<T>(dom: &Dom, pick: impl Fn(&CssProperty) -> Option<T>) -> Option<T> {
    dom.root
        .style
        .iter_inline_properties()
        .filter(|(_, conds)| conds.as_ref().is_empty())
        .filter_map(|(p, _)| pick(p))
        .last()
}

fn tab_doms(header: &Dom) -> Vec<&Dom> {
    header
        .children
        .as_ref()
        .iter()
        .filter(|c| {
            c.root
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|k| matches!(k.as_class(), Some(ACTIVE) | Some(NOT_ACTIVE)))
        })
        .collect()
}

#[test]
fn a_tab_button_has_the_house_radius_and_its_label_in_tracked_capitals() {
    let header = pills(1);
    for tab in tab_doms(&header) {
        let radius = declared(tab, |p| match p {
            CssProperty::BorderTopLeftRadius(v) => v.get_property().map(|v| v.inner.number.get()),
            _ => None,
        });
        assert_eq!(radius, Some(3.0), "the house radius, not a pill's");
        let size = declared(tab, |p| match p {
            CssProperty::FontSize(v) => v.get_property().map(|v| v.inner.number.get()),
            _ => None,
        })
        .expect("a size of its own");
        assert!((size - 11.5).abs() < 0.01, "11.5px capitals, not {size}");
        let track = declared(tab, |p| match p {
            CssProperty::LetterSpacing(v) => v.get_property().map(|v| v.inner.number.get()),
            _ => None,
        })
        .expect("tracked");
        assert!((track - 0.08).abs() < 0.001, "tracked .08em, not {track}");
        assert!(
            declared(tab, |p| matches!(p, CssProperty::TextTransform(_)).then_some(())).is_some(),
            "set in capitals"
        );
        let left = declared(tab, |p| match p {
            CssProperty::PaddingLeft(v) => v.get_property().map(|v| v.inner.number.get()),
            _ => None,
        })
        .expect("a left padding");
        let right = declared(tab, |p| match p {
            CssProperty::PaddingRight(v) => v.get_property().map(|v| v.inner.number.get()),
            _ => None,
        })
        .expect("a right padding");
        assert_eq!(left, 12.0, "12px before the label");
        assert!(
            (left - right - size * track).abs() < 0.01,
            "the right side takes the trailing tracking back: {left} / {right}"
        );
    }
}
