//! A tab bar takes the style it is given: Aurora (Firefox's curved tabs),
//! Rectangular (classic square tabs) or Pills (pill buttons on leather).
//!
//! - Aurora cuts EVERY tab with its curves, the selected one and the rest,
//!   so all tabs share one shape; only the selected one stands on the rule
//!   with its run-outs.
//! - Rectangular hangs no curves, in either theme.
//! - Pills sit on a leather strip that reaches the top of the bar; the strip
//!   closes on a 2px line along its foot, gold: a shimmer over aluminium.
//!   The selected pill wears the submit button's face.
//! - With no tab selected (an index past the last tab) no tab looks
//!   selected: no active class, no seam, no curves of a selected tab.
//! - A panel switched to a tab on the right slides right into place as it
//!   fades in, the one it replaces fading out; to a tab on the left,
//!   mirrored.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{
    props::{
        basic::animation::AnimationTiming,
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
    AzString, StringVec,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    widgets::{
        button::Button,
        tabs::{TabContent, TabHeader, TabStyle},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ACTIVE: &str = "__azul-native-tabs-tab-active";
const NOT_ACTIVE: &str = "__azul-native-tabs-tab-not-active";
const SEAMS: [&str; 2] = [
    "__azul-native-tabs-tab-norightborder",
    "__azul-native-tabs-tab-noleftborder",
];
const CURVES: [&str; 2] = ["__azul-native-tab-curve-left", "__azul-native-tab-curve-right"];
const RUNOUTS: [&str; 2] = ["__azul-native-tab-runout-left", "__azul-native-tab-runout-right"];
const RULE: &str = "__azul-native-tabs-rule";
const THEMES: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];

fn labels() -> StringVec {
    StringVec::from_vec(
        ["Start", "Insert", "Layout"]
            .iter()
            .map(|l| AzString::from(*l))
            .collect(),
    )
}

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| c.as_class() == Some(class))
}

/// The header's tabs, in order.
fn tabs(header: &Dom) -> Vec<&Dom> {
    header
        .children
        .as_ref()
        .iter()
        .filter(|c| has_class(c, ACTIVE) || has_class(c, NOT_ACTIVE))
        .collect()
}

/// Whether one of `node`'s children carries `class`.
fn has_child(node: &Dom, class: &str) -> bool {
    node.children.as_ref().iter().any(|c| has_class(c, class))
}

/// The last resting (unconditioned) background of `node`'s own style.
fn resting_layers(node: &Dom) -> Vec<StyleBackgroundContent> {
    node.root
        .style
        .iter_inline_properties()
        .filter(|(_, conds)| conds.as_ref().is_empty())
        .filter_map(|(p, _)| match p {
            CssProperty::BackgroundContent(v) => {
                v.get_property().map(|l| l.as_ref().to_vec())
            }
            _ => None,
        })
        .last()
        .unwrap_or_default()
}

fn builtin(name: &str) -> StyleBackgroundContent {
    StyleBackgroundContent::Image(AzString::from(format!("azul-builtin:{name}")))
}

#[test]
fn every_aurora_tab_is_cut_with_curves_and_only_the_selected_one_has_run_outs() {
    for theme in THEMES {
        let header = TabHeader::create(labels())
            .with_active_tab(1)
            .with_theme(theme)
            .with_style(TabStyle::Aurora)
            .dom();
        let tabs = tabs(&header);
        assert_eq!(tabs.len(), 3, "{theme:?}: the three tabs");
        for (i, tab) in tabs.iter().enumerate() {
            for curve in CURVES {
                assert!(has_child(tab, curve), "{theme:?}: tab {i} is cut with its {curve}");
            }
            let runouts = RUNOUTS.iter().filter(|r| has_child(tab, r)).count();
            if i == 1 {
                // Flora's rule is metal and eases into the selected tab's
                // feet; flat's is a hairline and needs no run-out.
                if theme == UiTheme::Flora {
                    assert!(runouts > 0, "flora: the selected tab eases the rule into its feet");
                }
            } else {
                assert_eq!(runouts, 0, "{theme:?}: tab {i} stands above the rule, no run-out");
            }
        }
    }
}

#[test]
fn flora_s_own_tab_bar_is_aurora_and_flat_s_is_rectangular() {
    let flora = TabHeader::create(labels()).with_theme(UiTheme::Flora).dom();
    assert!(tabs(&flora).iter().all(|t| has_child(t, CURVES[0])), "flora: every tab curved");
    let flat = TabHeader::create(labels()).with_theme(UiTheme::Flat).dom();
    assert!(tabs(&flat).iter().all(|t| !has_child(t, CURVES[0])), "flat: square tabs");
}

#[test]
fn a_rectangular_tab_bar_hangs_no_curves_in_either_theme() {
    for theme in THEMES {
        let header = TabHeader::create(labels())
            .with_theme(theme)
            .with_style(TabStyle::Rectangular)
            .dom();
        for tab in tabs(&header) {
            for part in CURVES.iter().chain(RUNOUTS.iter()) {
                assert!(!has_child(tab, part), "{theme:?}: a square tab has no {part}");
            }
        }
    }
}

#[test]
fn a_tab_bar_with_no_tab_selected_shows_none_selected() {
    for theme in THEMES {
        for style in [TabStyle::Aurora, TabStyle::Rectangular, TabStyle::Pills] {
            for none in [3, usize::MAX] {
                let header = TabHeader::create(labels())
                    .with_active_tab(none)
                    .with_theme(theme)
                    .with_style(style)
                    .dom();
                let tabs = tabs(&header);
                assert_eq!(tabs.len(), 3, "{theme:?} {style:?}: the three tabs");
                for (i, tab) in tabs.iter().enumerate() {
                    assert!(!has_class(tab, ACTIVE), "{theme:?} {style:?}: tab {i} is not active");
                    for seam in SEAMS {
                        assert!(
                            !has_class(tab, seam),
                            "{theme:?} {style:?} (index {none}): tab {i} has no {seam}: no \
                             tab is selected to join"
                        );
                    }
                    for runout in RUNOUTS {
                        assert!(!has_child(tab, runout), "{theme:?} {style:?}: no {runout}");
                    }
                }
            }
        }
    }
}

#[test]
fn pills_sit_on_leather_and_the_selected_pill_wears_the_submit_button_s_face() {
    for theme in THEMES {
        let header = TabHeader::create(labels())
            .with_active_tab(0)
            .with_theme(theme)
            .with_style(TabStyle::Pills)
            .dom();
        assert!(
            resting_layers(&header).contains(&builtin("leather")),
            "{theme:?}: the strip is leather: {:?}",
            resting_layers(&header)
        );
        let tabs = tabs(&header);
        for tab in &tabs {
            for part in CURVES.iter().chain(RUNOUTS.iter()) {
                assert!(!has_child(tab, part), "{theme:?}: a pill has no {part}");
            }
        }
        let submit = Button::create_submit(AzString::from("Send")).with_theme(theme).dom();
        assert_eq!(
            resting_layers(tabs[0]),
            resting_layers(&submit),
            "{theme:?}: the selected pill is the submit button's face"
        );
        assert_ne!(
            resting_layers(tabs[1]),
            resting_layers(tabs[0]),
            "{theme:?}: an unselected pill is not"
        );

        let rule: Vec<&Dom> = header
            .children
            .as_ref()
            .iter()
            .filter(|c| has_class(c, RULE))
            .collect();
        assert_eq!(rule.len(), 1, "{theme:?}: one rule closes the strip");
        let layers = resting_layers(rule[0]);
        assert!(layers.contains(&builtin("aluminium")), "{theme:?}: the rule is metal: {layers:?}");
        assert!(
            layers.iter().any(|l| matches!(l, StyleBackgroundContent::LinearGradient(_))),
            "{theme:?}: with the gold shimmer laid over it: {layers:?}"
        );
    }
}

/// `dom` in a 480 x 200 window at the top of a margin-less body, laid out
/// and its frame prepared (which renders the materials).
fn laid_out(dom: Dom) -> LayoutWindow {
    let body = Dom::create_body().with_css("margin: 0px;").with_child(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(480.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(body),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the bar lays out");
    lw.prepare_frame_content();
    lw
}

fn rects_of(lw: &LayoutWindow, class: &str) -> Vec<LogicalRect> {
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let nodes = result.styled_dom.node_data.as_container();
    (0..nodes.len())
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
        .collect()
}

fn contains(outer: &LogicalRect, inner: &LogicalRect) -> bool {
    inner.origin.x >= outer.origin.x - 0.5
        && inner.origin.y >= outer.origin.y - 0.5
        && inner.origin.x + inner.size.width <= outer.origin.x + outer.size.width + 0.5
        && inner.origin.y + inner.size.height <= outer.origin.y + outer.size.height + 0.5
}

/// The golden line is two CSS pixels tall - four device pixels at 2x, as
/// every length is scaled - and lies along the strip's foot; the leather is
/// drawn over the whole strip, from the top of the window, under every
/// pill.
#[test]
fn the_pill_strip_is_leather_to_the_top_and_closes_on_a_two_pixel_line() {
    for theme in THEMES {
        let lw = laid_out(
            TabHeader::create(labels())
                .with_active_tab(1)
                .with_theme(theme)
                .with_style(TabStyle::Pills)
                .dom(),
        );
        let header = rects_of(&lw, "__azul-native-tabs-header")[0];
        assert!(header.origin.y.abs() < 0.5, "{theme:?}: the strip starts at the top: {header:?}");
        let rule = rects_of(&lw, RULE);
        assert_eq!(rule.len(), 1, "{theme:?}: one rule");
        let rule = rule[0];
        assert!((rule.size.height - 2.0).abs() < 0.01, "{theme:?}: a 2px line: {rule:?}");
        assert!(
            (rule.origin.y + rule.size.height - (header.origin.y + header.size.height)).abs() < 0.5,
            "{theme:?}: along the strip's foot: rule {rule:?}, strip {header:?}"
        );
        assert!((rule.size.width - header.size.width).abs() < 0.5, "{theme:?}: the whole width");

        let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
        let leather = result
            .display_list
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::Image { bounds, .. } => Some(*bounds.inner()),
                _ => None,
            })
            .find(|b| (b.size.width - header.size.width).abs() < 0.5 && b.size.height > 2.0)
            .unwrap_or_else(|| panic!("{theme:?}: the leather is drawn over the strip"));
        assert!(leather.origin.y.abs() < 0.5, "{theme:?}: from the top: {leather:?}");
        let mut pills = rects_of(&lw, ACTIVE);
        pills.extend(rects_of(&lw, NOT_ACTIVE));
        assert_eq!(pills.len(), 3);
        for pill in pills {
            assert!(contains(&leather, &pill), "{theme:?}: pill {pill:?} sits on the leather");
        }
    }
}

/// The text runs of a laid-out window: each glyph's pen x and the run's
/// font size.
fn glyph_xs(lw: &LayoutWindow) -> Vec<(f32, f32)> {
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text {
                glyphs,
                font_size_px,
                ..
            } => Some(glyphs.iter().map(|g| (g.point.x, *font_size_px)).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A flora tab is as wide as its capitals and its 8px sides, no wider,
/// and no curve hangs over a neighbour's label.
#[test]
fn a_flora_aurora_tab_is_as_wide_as_its_capitals_and_no_curve_covers_a_neighbour() {
    let lw = laid_out(
        TabHeader::create(labels())
            .with_active_tab(1)
            .with_theme(UiTheme::Flora)
            .dom(),
    );
    let glyphs = glyph_xs(&lw);
    let mut tabs = rects_of(&lw, NOT_ACTIVE);
    tabs.extend(rects_of(&lw, ACTIVE));
    assert_eq!(tabs.len(), 3);
    for tab in &tabs {
        let mine: Vec<(f32, f32)> = glyphs
            .iter()
            .copied()
            .filter(|(x, _)| *x >= tab.origin.x && *x < tab.origin.x + tab.size.width)
            .collect();
        assert!(!mine.is_empty(), "the tab {tab:?} paints its label");
        let first = mine.iter().map(|g| g.0).fold(f32::INFINITY, f32::min);
        let last = mine.iter().map(|g| g.0).fold(f32::NEG_INFINITY, f32::max);
        let em = mine[0].1;
        assert!(
            first - tab.origin.x <= 8.0 + 1.5,
            "the label starts 8px in, at {first:.1} in {tab:?}"
        );
        assert!(
            tab.origin.x + tab.size.width - last <= 8.0 + 1.2 * em,
            "the tab ends 8px after its last capital (at {last:.1}): {tab:?}"
        );
    }
    let mut curves = rects_of(&lw, CURVES[0]);
    curves.extend(rects_of(&lw, CURVES[1]));
    for curve in curves {
        // The curve's foot spans its whole width at the bottom; its upper
        // half, where the labels are, lies within its inner half.
        let inner = LogicalRect::new(
            azul_core::geom::LogicalPosition::new(curve.origin.x + curve.size.width / 4.0, curve.origin.y),
            LogicalSize::new(curve.size.width / 2.0, curve.size.height),
        );
        for tab in &tabs {
            let overlaps_tab =
                inner.origin.x < tab.origin.x + tab.size.width && tab.origin.x < inner.origin.x + inner.size.width;
            if !overlaps_tab {
                continue;
            }
            let label_glyphs_under = glyphs.iter().filter(|(x, _)| {
                *x >= tab.origin.x
                    && *x < tab.origin.x + tab.size.width
                    && *x >= inner.origin.x
                    && *x < inner.origin.x + inner.size.width
            });
            assert_eq!(
                label_glyphs_under.count(),
                0,
                "a curve {curve:?} hangs over the label of {tab:?}"
            );
        }
    }
}

/// The `@keyframes` blocks the panel ships, by name.
fn keyframes<'a>(dom: &'a Dom, name: &str) -> Option<&'a azul_css::css::Keyframes> {
    fn walk<'a>(dom: &'a Dom, name: &str) -> Option<&'a azul_css::css::Keyframes> {
        dom.css
            .as_ref()
            .iter()
            .flat_map(|css| css.keyframes.as_ref().iter())
            .find(|k| k.name.as_str() == name)
            .or_else(|| dom.children.as_ref().iter().find_map(|c| walk(c, name)))
    }
    walk(dom, name)
}

/// The node of `dom` with `class`.
fn find<'a>(dom: &'a Dom, class: &str) -> Option<&'a Dom> {
    if has_class(dom, class) {
        return Some(dom);
    }
    dom.children.as_ref().iter().find_map(|c| find(c, class))
}

/// The first animation name `node` declares for `ty`.
fn animation_name(node: &Dom, ty: CssPropertyType) -> Option<String> {
    node.root
        .style
        .iter_inline_properties()
        .filter(|(p, _)| p.get_type() == ty)
        .find_map(|(p, _)| match p {
            CssProperty::AnimationIn(v) | CssProperty::AnimationOut(v) => v
                .get_property()
                .and_then(|l| l.as_ref().first().map(|a| a.name.as_str().to_string())),
            _ => None,
        })
}

/// `(start, end)` of the x shift and the opacity the engine compiles from
/// `name`.
fn motion(dom: &Dom, name: &str) -> ((f32, f32), (f32, f32)) {
    let kf = keyframes(dom, name).unwrap_or_else(|| panic!("the {name} keyframes"));
    let track = azul_layout::window::compile_keyframes_track(
        kf,
        LogicalRect::zero(),
        0.2,
        AnimationTiming::EaseOut,
    );
    let ends = |v: &[(f32, f32)]| (v.first().map_or(0.0, |p| p.1), v.last().map_or(0.0, |p| p.1));
    (ends(&track.translate_x), ends(&track.opacity))
}

/// The panel that leaves fades where it is: its exit is the style it was
/// built with (the engine runs a departing node's `-azul-animation-out`
/// from the old cascade), so it cannot know which way the next switch goes.
/// The panel that comes in carries the direction: to a tab on the right it
/// slides right into place (in from the left) as it fades in, to a tab on
/// the left it slides left.
#[test]
fn a_panel_switched_to_a_tab_on_the_right_slides_right_and_fades() {
    let panel = TabContent::new(Dom::create_div()).with_switch(0, 2).dom();
    let group = find(&panel, "__azul-native-tabs-content").expect("the content group");
    let enter = animation_name(group, CssPropertyType::AnimationIn).expect("it slides in");
    let leave = animation_name(group, CssPropertyType::AnimationOut).expect("it fades out");
    let ((in_from, in_to), (in_o0, in_o1)) = motion(&panel, &enter);
    assert!(in_from < 0.0 && in_to.abs() < 0.01, "in from the left: {in_from} -> {in_to}");
    assert!(in_o0 < in_o1, "fading in: {in_o0} -> {in_o1}");
    let ((out_from, out_to), (out_o0, out_o1)) = motion(&panel, &leave);
    assert!(out_from.abs() < 0.01 && out_to.abs() < 0.01, "it leaves in place");
    assert!(out_o0 > out_o1, "fading out: {out_o0} -> {out_o1}");

    let back = TabContent::new(Dom::create_div()).with_switch(2, 0).dom();
    let group = find(&back, "__azul-native-tabs-content").expect("the content group");
    let enter = animation_name(group, CssPropertyType::AnimationIn).expect("it slides in");
    let ((in_from, in_to), _) = motion(&back, &enter);
    assert!(
        in_from > 0.0 && in_to.abs() < 0.01,
        "to a tab on the left it comes in from the right: {in_from} -> {in_to}"
    );

    let still = TabContent::new(Dom::create_div()).dom();
    let group = find(&still, "__azul-native-tabs-content").expect("the content group");
    assert!(
        animation_name(group, CssPropertyType::AnimationIn).is_none(),
        "a panel nobody switched does not move"
    );
}
