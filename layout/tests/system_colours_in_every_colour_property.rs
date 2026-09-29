//! A `system:` colour keyword works in EVERY colour-valued property.
//!
//! `system:<slot>` (`system:accent`, `system:text`, ...) names a colour of the
//! desktop palette instead of one hex value that is right in one theme and
//! wrong in the other. It was accepted by `color`, the borders and the
//! backgrounds only; every other colour property rejected it, so the whole
//! declaration was dropped - a `caret-color: system:accent` fell back to the
//! text colour, a `box-shadow: 0 0 4px system:accent` painted no shadow at
//! all. And a property that did take the keyword but whose getter forgot to
//! resolve it (the `-azul-scrollbar-*` parts) painted the transparent token.
//!
//! Each row writes the keyword into one property, cascades the DOM under the
//! macOS light AND dark presets, reads the value back through the getter the
//! renderer reads it with, and wants the accent of THAT theme - the two
//! presets disagree on it, so a keyword resolved to one fixed colour cannot
//! pass.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
    window::DarkLightMode,
};
use azul_css::{
    dynamic_selector::{resolve_system_color_token, DynamicSelectorContext},
    props::{
        basic::{
            color::{ColorOrSystem, ColorU, SystemColorRef},
            PhysicalSize,
        },
        style::{filter::StyleFilter, StyleBackgroundContent},
    },
    system::{defaults, SystemStyle},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::{display_list::DisplayListItem, getters},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const THEMES: [DarkLightMode; 2] = [DarkLightMode::Light, DarkLightMode::Dark];

/// The div every row styles: `body(0) > div(1) > text(2)`.
const DIV: NodeId = NodeId::new(1);

fn preset(theme: DarkLightMode) -> Arc<SystemStyle> {
    Arc::new(match theme {
        DarkLightMode::Light => defaults::macos_modern_light(),
        DarkLightMode::Dark => defaults::macos_modern_dark(),
    })
}

/// The context a window evaluates its cascade against under `style`.
fn context(style: &SystemStyle) -> DynamicSelectorContext {
    DynamicSelectorContext::from_system_style(style).with_viewport(800.0, 600.0)
}

/// What `system:accent` must come out as under `theme`: the cascade's own
/// context answers it, so the expectation is never restated here.
fn accent(theme: DarkLightMode) -> ColorU {
    context(&preset(theme)).system_color(SystemColorRef::Accent)
}

/// One DOM cascaded under one theme.
struct Probe {
    sd: StyledDom,
    style: Arc<SystemStyle>,
}

impl Probe {
    fn new(css: &str, theme: DarkLightMode) -> Self {
        let style = preset(theme);
        let dom = Dom::create_body().with_child(
            Dom::create_div()
                .with_css(css)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("ink")),
        );
        let sd = StyledDom::create_from_dom_with_context(dom, Some(context(&style)));
        Self { sd, style }
    }

    fn state(&self) -> StyledNodeState {
        self.sd.styled_nodes.as_container()[DIV].styled_node_state
    }
}

/// A `ColorOrSystem` as the colour a renderer would be handed: an
/// unresolved reference comes back as its token, which never equals a
/// palette colour, so the row fails and prints what it saw.
const fn as_painted(c: ColorOrSystem) -> ColorU {
    match c {
        ColorOrSystem::Color(c) => c,
        ColorOrSystem::System(r) => r.to_color_token(),
    }
}

type Reader = fn(&Probe) -> Option<ColorU>;

/// `(property, declaration, how the renderer reads it)`.
///
/// The rows above the marker took `system:` before this test existed and
/// stay as the controls. The rows below it failed - all but the
/// `column-rule` shorthand, which borrows the border parser.
const ROWS: &[(&str, &str, Reader)] = &[
    ("color", "color: system:accent;", |p| {
        Some(
            getters::get_style_properties(
                &p.sd,
                DIV,
                Some(&p.style),
                PhysicalSize::new(800.0, 600.0),
            )
            .color,
        )
    }),
    ("color (inherited by the text)", "color: system:accent;", |p| {
        Some(
            getters::get_style_properties(
                &p.sd,
                NodeId::new(2),
                Some(&p.style),
                PhysicalSize::new(800.0, 600.0),
            )
            .color,
        )
    }),
    ("background-color", "background-color: system:accent;", |p| {
        Some(getters::get_background_color(&p.sd, DIV, &p.state()))
    }),
    ("background", "background: system:accent;", |p| {
        Some(getters::get_background_color(&p.sd, DIV, &p.state()))
    }),
    ("fill (the SVG spelling of background-color)", "fill: system:accent;", |p| {
        Some(getters::get_background_color(&p.sd, DIV, &p.state()))
    }),
    (
        "background-image gradient stop",
        "background-image: linear-gradient(system:accent, system:accent);",
        |p| {
            getters::get_background_contents(&p.sd, DIV, &p.state())
                .into_iter()
                .find_map(|layer| match layer {
                    StyleBackgroundContent::LinearGradient(g) => {
                        g.stops.as_ref().first().map(|s| as_painted(s.color))
                    }
                    _ => None,
                })
        },
    ),
    ("border (shorthand)", "border: 2px solid system:accent;", |p| {
        getters::get_border_info(&p.sd, DIV, &p.state())
            .colors
            .top
            .and_then(|v| v.get_property().copied())
            .map(|c| c.inner)
    }),
    (
        "border-left-color",
        "border-style: solid; border-width: 2px; border-left-color: system:accent;",
        |p| {
            getters::get_border_info(&p.sd, DIV, &p.state())
                .colors
                .left
                .and_then(|v| v.get_property().copied())
                .map(|c| c.inner)
        },
    ),
    (
        "stroke (the SVG spelling of border-color)",
        "border-style: solid; border-width: 2px; stroke: system:accent;",
        |p| {
            getters::get_border_info(&p.sd, DIV, &p.state())
                .colors
                .bottom
                .and_then(|v| v.get_property().copied())
                .map(|c| c.inner)
        },
    ),
    // ---- the marker: the rows below failed before `system:` worked everywhere ----
    ("caret-color", "caret-color: system:accent;", |p| {
        Some(getters::get_caret_style(&p.sd, Some(DIV)).color)
    }),
    (
        "-azul-selection-background-color",
        "-azul-selection-background-color: system:accent;",
        |p| Some(getters::get_selection_style(&p.sd, Some(DIV), Some(&p.style)).bg_color),
    ),
    ("-azul-selection-color", "-azul-selection-color: system:accent;", |p| {
        getters::get_selection_style(&p.sd, Some(DIV), Some(&p.style)).text_color
    }),
    (
        "scrollbar-color (thumb)",
        "scrollbar-color: system:accent system:accent;",
        |p| {
            Some(
                getters::get_scrollbar_style(&p.sd, DIV, &p.state(), Some(&*p.style))
                    .thumb_color,
            )
        },
    ),
    (
        "scrollbar-color (track)",
        "scrollbar-color: system:accent system:accent;",
        |p| {
            Some(
                getters::get_scrollbar_style(&p.sd, DIV, &p.state(), Some(&*p.style))
                    .track_color,
            )
        },
    ),
    ("-azul-scrollbar-thumb", "-azul-scrollbar-thumb: system:accent;", |p| {
        Some(getters::get_scrollbar_style(&p.sd, DIV, &p.state(), Some(&*p.style)).thumb_color)
    }),
    ("-azul-scrollbar-track", "-azul-scrollbar-track: system:accent;", |p| {
        Some(getters::get_scrollbar_style(&p.sd, DIV, &p.state(), Some(&*p.style)).track_color)
    }),
    ("-azul-scrollbar-button", "-azul-scrollbar-button: system:accent;", |p| {
        Some(getters::get_scrollbar_style(&p.sd, DIV, &p.state(), Some(&*p.style)).button_color)
    }),
    ("-azul-scrollbar-corner", "-azul-scrollbar-corner: system:accent;", |p| {
        Some(getters::get_scrollbar_style(&p.sd, DIV, &p.state(), Some(&*p.style)).corner_color)
    }),
    ("box-shadow (left)", "box-shadow: 0 0 4px system:accent;", |p| {
        getters::get_box_shadow_left(&p.sd, DIV, &p.state()).map(|s| s.color)
    }),
    ("box-shadow (bottom)", "box-shadow: 0 0 4px system:accent;", |p| {
        getters::get_box_shadow_bottom(&p.sd, DIV, &p.state()).map(|s| s.color)
    }),
    ("-azul-box-shadow-top", "-azul-box-shadow-top: 0 0 4px system:accent;", |p| {
        getters::get_box_shadow_top(&p.sd, DIV, &p.state()).map(|s| s.color)
    }),
    ("text-shadow", "text-shadow: 1px 1px 2px system:accent;", |p| {
        getters::get_text_shadow(&p.sd, DIV, &p.state()).map(|s| s.color)
    }),
    ("filter: drop-shadow()", "filter: drop-shadow(1px 1px 2px system:accent);", |p| {
        getters::get_filter(&p.sd, DIV, &p.state()).and_then(|v| {
            v.as_ref().iter().find_map(|f| match f {
                StyleFilter::DropShadow(s) => Some(s.color),
                _ => None,
            })
        })
    }),
    ("filter: flood()", "filter: flood(system:accent);", |p| {
        getters::get_filter(&p.sd, DIV, &p.state()).and_then(|v| {
            v.as_ref().iter().find_map(|f| match f {
                StyleFilter::Flood(c) => Some(*c),
                _ => None,
            })
        })
    }),
    ("backdrop-filter: flood()", "backdrop-filter: flood(system:accent);", |p| {
        getters::get_backdrop_filter(&p.sd, DIV, &p.state()).and_then(|v| {
            v.as_ref().iter().find_map(|f| match f {
                StyleFilter::Flood(c) => Some(*c),
                _ => None,
            })
        })
    }),
    // No painter reads column rules yet, so there is no renderer getter to
    // go through: the row pins that the keyword survives the parse as the
    // token the one resolver turns into the theme's colour.
    ("column-rule-color", "column-rule-color: system:accent;", column_rule_color),
    ("column-rule (shorthand)", "column-rule: 1px solid system:accent;", column_rule_color),
];

fn column_rule_color(p: &Probe) -> Option<ColorU> {
    let cache = p.sd.get_css_property_cache();
    let nd = &p.sd.node_data.as_container()[DIV];
    cache
        .get_column_rule_color(nd, &DIV, &p.state())
        .and_then(|v| v.get_property().copied())
        .map(|c| resolve_system_color_token(c.inner, cache.dynamic_context.as_deref()))
}

#[test]
fn every_colour_property_resolves_a_system_keyword_to_the_theme_it_renders_in() {
    let mut failures = Vec::new();
    for theme in THEMES {
        let want = accent(theme);
        for (property, css, read) in ROWS {
            let got = read(&Probe::new(css, theme));
            if got != Some(want) {
                failures.push(format!(
                    "{theme:?} {property}: `{css}` read back {got:?}, want Some({want:?})"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} rows do not paint the theme's accent:\n{}",
        failures.len(),
        ROWS.len() * THEMES.len(),
        failures.join("\n")
    );
}

/// The SVG presentation ATTRIBUTES are the third spelling of `fill` and
/// `stroke` (after the CSS property and the `style` attribute), translated
/// by the XML loader rather than the CSS parser.
#[test]
fn svg_fill_and_stroke_attributes_take_a_system_keyword() {
    for theme in THEMES {
        let want = accent(theme);
        let parsed = azul_layout::xml::parse_xml(
            r#"<svg viewBox="0 0 8 8" width="8" height="8">
                 <rect fill="system:accent" stroke="system:accent" stroke-width="1"
                       x="0" y="0" width="8" height="8"/>
               </svg>"#,
        )
        .expect("the fixture parses");
        let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
        let sd = StyledDom::create_from_dom_with_context(dom, Some(context(&preset(theme))));
        let rect = sd
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|nd| matches!(nd.node_type, NodeType::SvgRect))
            .map(NodeId::new)
            .expect("the <rect> is a node");
        let state = sd.styled_nodes.as_container()[rect].styled_node_state;

        assert_eq!(
            getters::get_background_color(&sd, rect, &state),
            want,
            "{theme:?}: fill=\"system:accent\""
        );
        assert_eq!(
            getters::get_border_info(&sd, rect, &state)
                .colors
                .top
                .and_then(|v| v.get_property().copied())
                .map(|c| c.inner),
            Some(want),
            "{theme:?}: stroke=\"system:accent\""
        );
    }
}

/// Lay `body > div(css) > text` out in a real window under `theme` and hand
/// back its display list: the shadows and filters the renderers are given.
fn display_list(css: &str, theme: DarkLightMode) -> Vec<DisplayListItem> {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css(css)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("ink")),
    );
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    // Before the first layout: a system style handed over afterwards is
    // invisible to the cascade that already ran.
    lw.set_system_style(preset(theme));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    ws.theme = match theme {
        DarkLightMode::Light => DarkLightMode::Light,
        DarkLightMode::Dark => DarkLightMode::Dark,
    };
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();

    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("a layout result for the root DOM")
        .display_list
        .items
        .clone()
}

/// The display list reads the shadows and the filters too; what it hands
/// the renderers must be the resolved colour, never the token.
#[test]
fn the_display_list_hands_the_renderers_resolved_shadow_and_filter_colours() {
    let css = "width: 40px; height: 20px; position: relative; z-index: 1; \
               box-shadow: 0 0 4px system:accent; \
               text-shadow: 1px 1px 2px system:accent; \
               filter: flood(system:accent); \
               backdrop-filter: flood(system:accent);";
    for theme in THEMES {
        let want = accent(theme);
        let items = display_list(css, theme);

        let box_shadows: Vec<ColorU> = items
            .iter()
            .filter_map(|i| match i {
                DisplayListItem::BoxShadow { shadow, .. } => Some(shadow.color),
                _ => None,
            })
            .collect();
        let text_shadows: Vec<ColorU> = items
            .iter()
            .filter_map(|i| match i {
                DisplayListItem::PushTextShadow { shadow } => Some(shadow.color),
                _ => None,
            })
            .collect();
        let floods = |backdrop: bool| -> Vec<ColorU> {
            items
                .iter()
                .filter_map(|i| match (i, backdrop) {
                    (DisplayListItem::PushFilter { filters, .. }, false)
                    | (DisplayListItem::PushBackdropFilter { filters, .. }, true) => Some(filters),
                    _ => None,
                })
                .flatten()
                .filter_map(|f| match f {
                    StyleFilter::Flood(c) => Some(*c),
                    _ => None,
                })
                .collect()
        };

        for (what, got) in [
            ("box-shadow", box_shadows),
            ("text-shadow", text_shadows),
            ("filter: flood()", floods(false)),
            ("backdrop-filter: flood()", floods(true)),
        ] {
            assert!(
                !got.is_empty() && got.iter().all(|c| *c == want),
                "{theme:?}: every {what} item must carry the theme's accent {want:?}; got {got:?}"
            );
        }
    }
}
