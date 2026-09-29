//! Every themed widget follows the window's theme in BOTH of its looks.
//!
//! `widgets::theme_contrast` (in the crate) renders the default look of every
//! widget under the macOS light and dark presets and fails on text that did
//! not follow the theme. The default look is Flat, so a widget's FLORA look
//! was never rendered by anything: a flora face with a light-only colour, or
//! a dark twin pushed before its light value, would ship unnoticed.
//!
//! This file asks the same two questions of both looks, through the public
//! API only (`Widget::with_theme(..)`):
//!
//! * CONTRAST: each visible text node's ink, composited over the stack of backgrounds behind it,
//!   reads at 2:1 or better, and in the dark theme text never sits on a light neutral surface (a
//!   "light island");
//! * PAIRS: every dark twin a node declares has its light half declared EARLIER, in the same
//!   pseudo-state (inline declarations resolve last-match-wins, so a twin before its light value is
//!   dead, and a twin with no light half leaves the light window on the UA default).

use std::sync::Arc;

use azul_core::{
    dom::{Dom, NodeId, NodeType},
    styled_dom::StyledDom,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, DynamicSelectorContext},
    props::{
        basic::{
            color::{ColorOrSystem, ColorU, SystemColorRef},
            PhysicalSize,
        },
        layout::LayoutDisplay,
        style::StyleBackgroundContent,
    },
    system::{defaults, SystemStyle, Theme},
    AzString,
};
use azul_layout::{solver3::getters, widgets::themes::UiTheme};

struct Probe {
    theme: Theme,
    style: Arc<SystemStyle>,
    ctx: DynamicSelectorContext,
}

fn probe(theme: Theme) -> Probe {
    let style = Arc::new(match theme {
        Theme::Light => defaults::macos_modern_light(),
        Theme::Dark => defaults::macos_modern_dark(),
    });
    let ctx = DynamicSelectorContext::from_system_style(&style).with_viewport(800.0, 600.0);
    Probe { theme, style, ctx }
}

type Rgb = [f32; 3];

fn rgb(c: ColorU) -> Rgb {
    [f32::from(c.r), f32::from(c.g), f32::from(c.b)]
}

fn to_color(c: Rgb) -> ColorU {
    ColorU {
        r: c[0].round() as u8,
        g: c[1].round() as u8,
        b: c[2].round() as u8,
        a: 255,
    }
}

fn over(top: ColorU, base: Rgb) -> Rgb {
    let a = f32::from(top.a) / 255.0;
    let t = rgb(top);
    [
        t[0] * a + base[0] * (1.0 - a),
        t[1] * a + base[1] * (1.0 - a),
        t[2] * a + base[2] * (1.0 - a),
    ]
}

fn luminance(c: Rgb) -> f32 {
    let lin = |v: f32| {
        let v = v / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn chroma(c: Rgb) -> f32 {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    (max - min) / 255.0
}

/// The colour one background layer contributes: a gradient counts as the
/// average of its stops, an image contributes nothing.
fn layer_color(layer: &StyleBackgroundContent, p: &Probe) -> Option<ColorU> {
    let stop = |c: &ColorOrSystem| match c {
        ColorOrSystem::Color(c) => *c,
        ColorOrSystem::System(r) => p.ctx.system_color(*r),
    };
    let average = |colors: Vec<ColorU>| -> Option<ColorU> {
        if colors.is_empty() {
            return None;
        }
        let n = colors.len() as f32;
        let sum = colors.iter().fold([0.0_f32; 4], |acc, c| {
            [
                acc[0] + f32::from(c.r),
                acc[1] + f32::from(c.g),
                acc[2] + f32::from(c.b),
                acc[3] + f32::from(c.a),
            ]
        });
        Some(ColorU {
            r: (sum[0] / n).round() as u8,
            g: (sum[1] / n).round() as u8,
            b: (sum[2] / n).round() as u8,
            a: (sum[3] / n).round() as u8,
        })
    };
    match layer {
        StyleBackgroundContent::Color(c) => Some(*c),
        StyleBackgroundContent::SystemColor(r) => Some(p.ctx.system_color(*r)),
        StyleBackgroundContent::LinearGradient(g) => {
            average(g.stops.as_ref().iter().map(|s| stop(&s.color)).collect())
        }
        StyleBackgroundContent::RadialGradient(g) => {
            average(g.stops.as_ref().iter().map(|s| stop(&s.color)).collect())
        }
        StyleBackgroundContent::ConicGradient(g) => {
            average(g.stops.as_ref().iter().map(|s| stop(&s.color)).collect())
        }
        StyleBackgroundContent::Image(_) => None,
    }
}

fn root_path(sd: &StyledDom, node: NodeId) -> Vec<NodeId> {
    let hierarchy = sd.node_hierarchy.as_container();
    let mut path = vec![node];
    let mut cur = hierarchy[node].parent_id();
    while let Some(n) = cur {
        path.push(n);
        cur = hierarchy[n].parent_id();
    }
    path.reverse();
    path
}

fn is_visible(sd: &StyledDom, path: &[NodeId]) -> bool {
    let states = sd.styled_nodes.as_container();
    path.iter().all(|&n| {
        !matches!(
            getters::get_display_property(sd, Some(n)),
            getters::MultiValue::Exact(LayoutDisplay::None)
        ) && getters::get_opacity(sd, n, &states[n].styled_node_state) > 0.0
    })
}

fn styled(dom: Dom, p: &Probe) -> StyledDom {
    StyledDom::create_from_dom_with_context(
        Dom::create_body().with_child(dom),
        Some(p.ctx.clone()),
    )
}

fn seen(sd: &StyledDom, text: NodeId, p: &Probe) -> (Rgb, Rgb) {
    let states = sd.styled_nodes.as_container();
    let mut bg = rgb(p.ctx.system_color(SystemColorRef::WindowBackground));
    for n in root_path(sd, text) {
        for layer in getters::get_background_contents(sd, n, &states[n].styled_node_state) {
            if let Some(c) = layer_color(&layer, p) {
                bg = over(c, bg);
            }
        }
    }
    let ink = getters::get_style_properties(
        sd,
        text,
        Some(&p.style),
        PhysicalSize::new(800.0, 600.0),
    )
    .color;
    (over(ink, bg), bg)
}

fn contrast_findings(name: &str, dom: Dom, p: &Probe) -> Vec<String> {
    let sd = styled(dom, p);
    let nodes = sd.node_data.as_container();
    let mut out = Vec::new();
    for i in 0..nodes.len() {
        let id = NodeId::new(i);
        let NodeType::Text(text) = nodes[id].get_node_type() else {
            continue;
        };
        let label = text.as_str();
        if label.trim().is_empty() || !is_visible(&sd, &root_path(&sd, id)) {
            continue;
        }
        let (fg, bg) = seen(&sd, id, p);
        let ratio = contrast(fg, bg);
        if ratio < 2.0 {
            out.push(format!(
                "{name} ({:?}): {label:?} reads {ratio:.2}:1 - ink {:?} on {:?}",
                p.theme,
                to_color(fg),
                to_color(bg),
            ));
        } else if p.theme == Theme::Dark && luminance(bg) > 0.45 && chroma(bg) < 0.25 {
            out.push(format!(
                "{name} (Dark): {label:?} sits on the light surface {:?} - a light island",
                to_color(bg),
            ));
        }
    }
    out
}

/// Every dark twin in `dom` without an earlier light half in the same
/// pseudo-state.
fn pair_findings(name: &str, dom: &Dom, path: &str, out: &mut Vec<String>) {
    let props: Vec<CssPropertyWithConditions> = dom
        .root
        .style
        .iter_inline_properties()
        .map(|(p, conds)| CssPropertyWithConditions {
            property: p.clone(),
            apply_if: conds.clone(),
        })
        .collect();
    for (i, twin) in props.iter().enumerate() {
        if !twin.is_dark_twin() {
            continue;
        }
        let ty = twin.property.get_type();
        let states = twin.pseudo_state_conditions();
        let light = props.iter().position(|p| {
            p.property.get_type() == ty && p.is_light_half() && p.pseudo_state_conditions() == states
        });
        match light {
            None => out.push(format!(
                "{name}: node {path} has a dark twin for {ty:?} (states {states:?}) with no light half"
            )),
            Some(j) if j > i => out.push(format!(
                "{name}: node {path} pushes the dark twin for {ty:?} before its light value"
            )),
            Some(_) => {}
        }
    }
    for (i, child) in dom.children.as_ref().iter().enumerate() {
        pair_findings(name, child, &format!("{path}/{i}"), out);
    }
}

/// Render each widget under both presets and walk its declarations; fail
/// with every finding at once.
fn assert_follow_the_theme(widgets: Vec<(String, Dom)>) {
    assert!(!widgets.is_empty(), "premise: something to check");
    let (light, dark) = (probe(Theme::Light), probe(Theme::Dark));
    let mut bad = Vec::new();
    for (name, dom) in widgets {
        pair_findings(&name, &dom, "root", &mut bad);
        bad.extend(contrast_findings(&name, dom.clone(), &light));
        bad.extend(contrast_findings(&name, dom, &dark));
    }
    assert!(
        bad.is_empty(),
        "{} finding(s):\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}

const LOOKS: [(&str, UiTheme); 2] = [("flat", UiTheme::Flat), ("flora", UiTheme::Flora)];

/// A guard on the guard: the pair walk must see both failure shapes.
#[test]
fn the_pair_walk_reports_a_missing_half_and_a_reversed_pair() {
    use azul_css::props::{property::CssProperty, style::StyleTextColor};
    let c = |v: u8| {
        CssProperty::const_text_color(StyleTextColor {
            inner: ColorU::rgb(v, v, v),
        })
    };
    let lone = Dom::create_div().with_css_props(
        vec![CssPropertyWithConditions::dark_theme(c(1))].into(),
    );
    let mut bad = Vec::new();
    pair_findings("fixture", &lone, "root", &mut bad);
    assert_eq!(bad.len(), 1, "{bad:?}");

    let reversed = Dom::create_div().with_css_props(
        vec![
            CssPropertyWithConditions::dark_theme(c(1)),
            CssPropertyWithConditions::simple(c(2)),
        ]
        .into(),
    );
    let mut bad = Vec::new();
    pair_findings("fixture", &reversed, "root", &mut bad);
    assert_eq!(bad.len(), 1, "{bad:?}");
}

#[test]
fn badges_read_in_both_themes_in_both_looks() {
    use azul_layout::widgets::badge::{Badge, BadgeKind};
    let mut widgets = Vec::new();
    for (look, theme) in LOOKS {
        for kind in [
            BadgeKind::Default,
            BadgeKind::Primary,
            BadgeKind::Success,
            BadgeKind::Danger,
            BadgeKind::Warning,
            BadgeKind::Info,
        ] {
            widgets.push((
                format!("{look} badge {kind:?}"),
                Badge::with_kind(AzString::from("New"), kind)
                    .with_theme(theme)
                    .dom(),
            ));
        }
    }
    assert_follow_the_theme(widgets);
}

#[test]
fn labels_read_in_both_themes_in_both_looks() {
    use azul_layout::widgets::label::Label;
    let widgets = LOOKS
        .iter()
        .map(|(look, theme)| {
            (
                format!("{look} label"),
                Label::create(AzString::from("Name")).with_theme(*theme).dom(),
            )
        })
        .collect();
    assert_follow_the_theme(widgets);
}

#[test]
fn dividers_pair_their_night_rule_in_both_looks() {
    use azul_layout::widgets::divider::{Divider, DividerOrientation};
    let mut widgets = Vec::new();
    for (look, theme) in LOOKS {
        for orientation in [DividerOrientation::Horizontal, DividerOrientation::Vertical] {
            widgets.push((
                format!("{look} divider {orientation:?}"),
                Divider::create_with_orientation(orientation)
                    .with_theme(theme)
                    .dom(),
            ));
        }
    }
    assert_follow_the_theme(widgets);
}

#[test]
fn spinners_pair_their_night_ink_in_both_looks() {
    use azul_layout::widgets::spinner::{Spinner, SpinnerStyle};
    let mut widgets = Vec::new();
    for (look, theme) in LOOKS {
        for style in [SpinnerStyle::Auto, SpinnerStyle::Spokes, SpinnerStyle::Ring] {
            widgets.push((
                format!("{look} spinner {style:?}"),
                Spinner::create()
                    .with_theme(theme)
                    .with_indicator(style)
                    .dom(),
            ));
            widgets.push((
                format!("{look} spinner {style:?} tracked"),
                Spinner::create()
                    .with_theme(theme)
                    .with_indicator(style)
                    .with_track_color(ColorU::rgb(200, 200, 200))
                    .dom(),
            ));
        }
    }
    assert_follow_the_theme(widgets);
}

#[test]
fn chips_read_in_both_themes_in_both_looks() {
    use azul_layout::widgets::chip::{Chip, ChipKind};
    let mut widgets = Vec::new();
    for (look, theme) in LOOKS {
        for kind in [
            ChipKind::Default,
            ChipKind::Primary,
            ChipKind::Success,
            ChipKind::Danger,
            ChipKind::Warning,
            ChipKind::Info,
        ] {
            widgets.push((
                format!("{look} chip {kind:?}"),
                Chip::with_kind(AzString::from("Rust"), kind)
                    .with_removable(true)
                    .with_theme(theme)
                    .dom(),
            ));
        }
    }
    assert_follow_the_theme(widgets);
}

#[test]
fn alerts_read_in_both_themes_in_both_looks() {
    use azul_layout::widgets::alert::{Alert, AlertKind};
    let mut widgets = Vec::new();
    for (look, theme) in LOOKS {
        for kind in [
            AlertKind::Info,
            AlertKind::Success,
            AlertKind::Warning,
            AlertKind::Danger,
        ] {
            widgets.push((
                format!("{look} alert {kind:?}"),
                Alert::with_kind(AzString::from("Message"), kind)
                    .with_dismissible(true)
                    .with_theme(theme)
                    .dom(),
            ));
        }
    }
    assert_follow_the_theme(widgets);
}

#[test]
fn cards_hold_readable_text_in_both_themes_in_both_looks() {
    use azul_layout::widgets::card::Card;
    let widgets = LOOKS
        .iter()
        .map(|(look, theme)| {
            (
                format!("{look} card + text"),
                Card::create(Dom::create_p_with_text("Body text"))
                    .with_theme(*theme)
                    .dom(),
            )
        })
        .collect();
    assert_follow_the_theme(widgets);
}
