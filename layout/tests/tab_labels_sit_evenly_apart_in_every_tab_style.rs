//! Tab labels sit evenly apart in every tab style.
//!
//! A selected Aurora tab hangs an 18px S curve off each side of its box. The
//! row is spaced so that every S lies in a gap of its own width, shared by
//! the two tabs beside it (Firefox's tabs overlap the same way), with 8px
//! between the S and each label:
//!
//! - Aurora, in both themes: every label 8px inside its tab, the tabs 18px
//!   apart, the first tab 18px in (room for its left curve).
//! - Rectangular hangs no curve, so it needs no room for one: 12px beside
//!   each label, 2px between the tabs, the first tab at the strip's edge.
//! - Flora's ribbon cuts its selected tab and its application button with
//!   the same curves, so its row is spaced as Aurora is.
//! - A label in tracked capitals gives its trailing tracking back: the text
//!   adds the tracking after the last capital too (as Chrome does), which
//!   would push the label half a tracking left of the tab's centre.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{
    props::property::CssProperty,
    AzString, StringVec,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    widgets::{
        ribbon::{Ribbon, RibbonAppButton, RibbonTab, RibbonTabVec},
        tabs::{TabHeader, TabStyle},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ACTIVE: &str = "__azul-native-tabs-tab-active";
const NOT_ACTIVE: &str = "__azul-native-tabs-tab-not-active";
const RIBBON_TAB: &str = "__azul-native-ribbon-tab";
const RIBBON_APP_BUTTON: &str = "__azul-native-ribbon-appbutton";

fn labels() -> StringVec {
    StringVec::from_vec(
        ["Start", "Insert", "Layout"]
            .iter()
            .map(|l| AzString::from(*l))
            .collect(),
    )
}

/// `dom` in a 640 x 240 window at the top left of a margin-less body.
fn laid_out(dom: Dom) -> LayoutWindow {
    let body = Dom::create_body().with_css("margin: 0px;").with_child(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 240.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(body),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the bar lays out");
    lw
}

/// The border boxes of the nodes carrying `class`, left to right.
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

/// Every glyph's pen x in the window.
fn glyph_xs(lw: &LayoutWindow) -> Vec<f32> {
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => {
                Some(glyphs.iter().map(|g| g.point.x).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

/// How far in from `tab`'s left edge its label's first glyph stands.
fn label_inset(glyphs: &[f32], tab: &LogicalRect) -> f32 {
    let first = glyphs
        .iter()
        .copied()
        .filter(|x| *x >= tab.origin.x && *x < tab.origin.x + tab.size.width)
        .fold(f32::INFINITY, f32::min);
    assert!(first.is_finite(), "the tab {tab:?} paints its label");
    first - tab.origin.x
}

/// The room between each tab and the next.
fn gaps(tabs: &[LogicalRect]) -> Vec<f32> {
    tabs.windows(2)
        .map(|w| w[1].origin.x - (w[0].origin.x + w[0].size.width))
        .collect()
}

fn header_tabs(lw: &LayoutWindow) -> Vec<LogicalRect> {
    let mut tabs = rects_of(lw, NOT_ACTIVE);
    tabs.extend(rects_of(lw, ACTIVE));
    tabs.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    tabs
}

#[test]
fn an_aurora_label_sits_8px_inside_its_tab_and_the_tabs_stand_a_curve_s_width_apart() {
    for theme in [UiTheme::Flora, UiTheme::Flat] {
        let lw = laid_out(
            TabHeader::create(labels())
                .with_active_tab(1)
                .with_theme(theme)
                .with_style(TabStyle::Aurora)
                .dom(),
        );
        let tabs = header_tabs(&lw);
        assert_eq!(tabs.len(), 3, "{theme:?}: three tabs");
        assert!(
            (tabs[0].origin.x - 18.0).abs() < 0.5,
            "{theme:?}: the first tab stands 18px in, room for its left curve: {:?}",
            tabs[0]
        );
        for gap in gaps(&tabs) {
            assert!((gap - 18.0).abs() < 0.5, "{theme:?}: the tabs are 18px apart, not {gap}");
        }
        let glyphs = glyph_xs(&lw);
        for tab in &tabs {
            let inset = label_inset(&glyphs, tab);
            assert!(
                (inset - 8.0).abs() < 1.0,
                "{theme:?}: the label sits 8px inside its tab, not {inset:.1}: {tab:?}"
            );
        }
    }
}

#[test]
fn a_rectangular_row_starts_at_the_strip_s_edge_with_its_tabs_2px_apart() {
    let lw = laid_out(
        TabHeader::create(labels())
            .with_active_tab(1)
            .with_theme(UiTheme::Flora)
            .with_style(TabStyle::Rectangular)
            .dom(),
    );
    let tabs = header_tabs(&lw);
    assert_eq!(tabs.len(), 3);
    assert!(
        tabs[0].origin.x.abs() < 0.5,
        "no spacer and no padding before the first tab: {:?}",
        tabs[0]
    );
    for gap in gaps(&tabs) {
        assert!((gap - 2.0).abs() < 0.5, "the tabs are 2px apart, not {gap}");
    }
    let glyphs = glyph_xs(&lw);
    for tab in &tabs {
        let inset = label_inset(&glyphs, tab);
        assert!((inset - 12.0).abs() < 1.0, "12px beside the label, not {inset:.1}: {tab:?}");
    }
}

#[test]
fn a_flora_ribbon_spaces_its_curved_tab_row_as_aurora() {
    let ribbon = Ribbon::new(RibbonTabVec::from_vec(vec![
        RibbonTab::new("HOME".into()),
        RibbonTab::new("SHARE".into()),
        RibbonTab::new("VIEW".into()),
    ]))
    .with_app_button(RibbonAppButton::new("FILE".into()))
    .with_active_tab(1)
    .with_theme(UiTheme::Flora)
    .with_available_width(640.0);
    let lw = laid_out(ribbon.dom_desktop());
    let mut row = rects_of(&lw, RIBBON_APP_BUTTON);
    row.extend(rects_of(&lw, RIBBON_TAB));
    row.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    assert_eq!(row.len(), 4, "the application button and three tabs");
    assert!(
        (row[0].origin.x - 18.0).abs() < 0.5,
        "the application button stands 18px in, room for its curve: {:?}",
        row[0]
    );
    for gap in gaps(&row) {
        assert!((gap - 18.0).abs() < 0.5, "the row's items are 18px apart, not {gap}");
    }
    let glyphs = glyph_xs(&lw);
    for item in &row {
        let inset = label_inset(&glyphs, item);
        assert!((inset - 8.0).abs() < 1.0, "the label sits 8px in, not {inset:.1}: {item:?}");
    }
}

/// The px a declaration list sets for one side, the last unconditioned
/// declaration winning.
fn declared_px(dom: &Dom, pick: impl Fn(&CssProperty) -> Option<f32>) -> Option<f32> {
    dom.root
        .style
        .iter_inline_properties()
        .filter(|(_, conds)| conds.as_ref().is_empty())
        .filter_map(|(p, _)| pick(p))
        .last()
}

fn padding_left(dom: &Dom) -> f32 {
    declared_px(dom, |p| match p {
        CssProperty::PaddingLeft(v) => v.get_property().map(|v| v.inner.number.get()),
        _ => None,
    })
    .unwrap_or(0.0)
}

fn padding_right(dom: &Dom) -> f32 {
    declared_px(dom, |p| match p {
        CssProperty::PaddingRight(v) => v.get_property().map(|v| v.inner.number.get()),
        _ => None,
    })
    .unwrap_or(0.0)
}

/// The tracking a node's label is set with, in px at its font size.
fn tracking_px(dom: &Dom) -> f32 {
    let size = declared_px(dom, |p| match p {
        CssProperty::FontSize(v) => v.get_property().map(|v| v.inner.number.get()),
        _ => None,
    })
    .expect("the tab sets its label's size");
    let em = declared_px(dom, |p| match p {
        CssProperty::LetterSpacing(v) => v.get_property().map(|v| v.inner.number.get()),
        _ => None,
    })
    .expect("the tab tracks its capitals");
    em * size
}

/// The tabs of a header DOM, in order.
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

/// The label is the tab's own text, so the tab takes the tracking back on
/// its right side: its padding there is a tracking less than on its left.
#[test]
fn a_tracked_tab_label_gives_its_trailing_tracking_back() {
    for style in [TabStyle::Aurora, TabStyle::Rectangular] {
        let header = TabHeader::create(labels())
            .with_active_tab(0)
            .with_theme(UiTheme::Flora)
            .with_style(style)
            .dom();
        for tab in tab_doms(&header) {
            let tracking = tracking_px(tab);
            assert!(tracking > 0.5, "{style:?}: premise, the capitals are tracked");
            let (left, right) = (padding_left(tab), padding_right(tab));
            assert!(
                (left - right - tracking).abs() < 0.01,
                "{style:?}: the right padding {right} is the left {left} less the tracking \
                 {tracking}"
            );
        }
    }
}
