//! A stylesheet wrapped in comment markers keeps its rules.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.2,
//! probe `css2` and sample 03, gap E-CSS-1): Outlook wraps every stylesheet in
//! `<!--` / `-->`, a relic of browsers that did not know `<style>`:
//!
//! ```html
//! <style><!--
//! p.MsoNormal { margin: 0cm; }
//! --></style>
//! ```
//!
//! Two layers lost those rules:
//!
//! - the CSS parser: CSS Syntax 3 (5.4.1 "consume a list of rules") ignores
//!   CDO / CDC tokens at the top level of a stylesheet, but azul's tokenizer
//!   stopped at the `<` and the parser dropped everything after it - so
//!   every `p.MsoNormal { margin: 0 }` was lost and each line of the mail
//!   sat 1em apart;
//! - the XML loaders: `<style>` is a raw-text element in HTML, but the XML
//!   tokenizer reads `<!-- .. -->` inside it as a COMMENT, which both loaders
//!   dropped - the style element's text was empty.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_css::props::basic::ColorU;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const RED: ColorU = ColorU {
    r: 0xc0,
    g: 0,
    b: 0,
    a: 255,
};

/// Outlook's shape: CDO, rules, a comment, CDC - and one rule after the CDC.
const OUTLOOK_SHEET: &str = "<!--\n\
/* Style Definitions */\n\
p.MsoNormal, li.MsoNormal { margin-top: 0cm; font-size: 11pt; }\n\
-->\n\
p.after { color: #c00000; }\n";

/// A mail whose only rule sits inside the comment markers.
const MAIL: &str = "<html><head><style><!--\n\
p.a { color: #c00000; }\n\
--></style></head><body><div><p class=\"a\">Outlook wraps its sheet</p></div></body></html>";

#[test]
fn the_css_parser_ignores_comment_markers_at_the_top_level() {
    let (css, _warnings) = azul_css::parser2::new_from_str(OUTLOOK_SHEET);
    let rules: Vec<_> = css.rules().collect();
    // `p.MsoNormal, li.MsoNormal` is one rule per selector, then `p.after`.
    assert_eq!(
        rules.len(),
        3,
        "the rules inside and after the comment markers are kept: {rules:#?}"
    );
    let declarations: usize = rules.iter().map(|r| r.declarations.as_ref().len()).sum();
    assert_eq!(declarations, 2 + 2 + 1, "{rules:#?}");
}

/// The colour every glyph of `styled` paints in, laid out in a window.
fn glyph_colours(styled: StyledDom) -> Vec<ColorU> {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 200.0);
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
            DisplayListItem::Text { color, glyphs, .. } if !glyphs.is_empty() => Some(*color),
            _ => None,
        })
        .collect()
}

fn assert_all_red(colours: &[ColorU], which: &str) {
    assert!(!colours.is_empty(), "{which}: the paragraph paints text");
    for c in colours {
        assert_eq!(
            *c, RED,
            "{which}: `p.a {{ color: #c00000 }}` inside the comment markers applies"
        );
    }
}

#[test]
fn the_tree_loader_keeps_a_style_elements_rules_inside_comment_markers() {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    assert_all_red(
        &glyph_colours(StyledDom::create_from_dom(dom)),
        "tree loader",
    );
}

#[test]
fn the_fast_loader_keeps_a_style_elements_rules_inside_comment_markers() {
    let styled = azul_layout::xml::parse_xml_to_styled_dom(MAIL).expect("the mail parses");
    assert_all_red(&glyph_colours(styled), "fast loader");
}
