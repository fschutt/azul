//! A light `color-scheme` card stays light in a dark window.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.5,
//! gap E-MODE): AzMail shows a DESIGNED html mail (colours, a table layout)
//! in a light "paper" card inside a dark app, the way Apple Mail and
//! Outlook do. The mail's own stylesheet may carry
//! `@media (prefers-color-scheme: dark)` blocks, and the UA defaults (a
//! link's colour) follow the mode - both were evaluated against the WINDOW
//! only (`DynamicSelector::Mode` against `DynamicSelectorContext::mode`), so
//! inside the light card the dark rules fired and a link turned the dark
//! mode's pale blue on white paper.
//!
//! CSS Color Adjust 1 gives an element its own `color-scheme`; azul treats
//! `color-scheme: light` / `dark` on a container like the embedding of a
//! document in that mode: inside it, `prefers-color-scheme` and every
//! mode-dependent UA default resolve in the container's mode.
//! `normal` and `light dark` follow the window. Inherited.
//!
//! Not compiled by the author (house rule); expected RED before the fix
//! (`color-scheme` is not a property azul parses yet).

use azul_core::{
    dom::DomId,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::{DarkLightMode, OptionDarkLightMode},
};
use azul_css::props::basic::ColorU;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const MAIL: &str = "<html><head><style>\
.x { color: #111111; }\
@media (prefers-color-scheme: dark) { .x { color: #eeeeee; } }\
</style></head><body>\
<p class=\"x\">outside</p>\
<div style=\"color-scheme: light; background: #ffffff;\">\
<p class=\"x\">inside</p>\
<p><a href=\"https://example.org\">link</a></p>\
</div></body></html>";

fn rgb(r: u8, g: u8, b: u8) -> ColorU {
    ColorU { r, g, b, a: 255 }
}

/// `(glyph count, colour)` of every text run, laid out in a DARK window.
fn runs_in_a_dark_window() -> Vec<(usize, ColorU)> {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    lw.mode = OptionDarkLightMode::Some(DarkLightMode::Dark);
    let mut ws = FullWindowState::default();
    ws.mode = DarkLightMode::Dark;
    ws.size.dimensions = LogicalSize::new(640.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the mail lays out");
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, color, .. } if !glyphs.is_empty() => {
                Some((glyphs.len(), *color))
            }
            _ => None,
        })
        .collect()
}

fn colour_of(runs: &[(usize, ColorU)], glyphs: usize, what: &str) -> ColorU {
    runs.iter()
        .find(|(n, _)| *n == glyphs)
        .map(|(_, c)| *c)
        .unwrap_or_else(|| panic!("{what} ({glyphs} glyphs) paints: {runs:?}"))
}

#[test]
fn outside_the_card_the_dark_rule_applies() {
    if azul_css::dynamic_selector::mode_pinned_by_env().is_some() {
        return; // the environment pins the mode; the window's is not in play
    }
    let runs = runs_in_a_dark_window();
    assert_eq!(
        colour_of(&runs, "outside".len(), "\"outside\""),
        rgb(0xee, 0xee, 0xee),
        "the dark window matches `prefers-color-scheme: dark`"
    );
}

#[test]
fn inside_a_light_card_the_dark_rule_does_not_apply() {
    if azul_css::dynamic_selector::mode_pinned_by_env().is_some() {
        return;
    }
    let runs = runs_in_a_dark_window();
    assert_eq!(
        colour_of(&runs, "inside".len(), "\"inside\""),
        rgb(0x11, 0x11, 0x11),
        "`color-scheme: light` on the card: `prefers-color-scheme: dark` does not match in it"
    );
}

#[test]
fn a_link_inside_a_light_card_takes_the_light_link_colour() {
    if azul_css::dynamic_selector::mode_pinned_by_env().is_some() {
        return;
    }
    let runs = runs_in_a_dark_window();
    assert_eq!(
        colour_of(&runs, "link".len(), "\"link\""),
        rgb(0x00, 0x00, 0xee),
        "the UA link colour of the card's mode (#0000EE), not the dark mode's #9E9EFF"
    );
}
