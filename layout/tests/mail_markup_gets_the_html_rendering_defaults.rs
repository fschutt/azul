//! Mail markup gets the HTML rendering defaults.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.2
//! and gap E-UA): mail HTML leans on the user-agent stylesheet - it writes
//! `<em>`, `<s>`, `<code>`, `<blockquote>` and `<a href>` and expects the
//! browser's look. azul's UA table (`azul_core::ua_css`) gave `em`/`i` no
//! italic, `s`/`del` no line-through, `code`/`pre` no monospace,
//! `blockquote` no margin and a link no colour. The expected values are the
//! HTML Living Standard's rendering section (15.3.3 flow content, 15.3.4
//! phrasing content):
//!
//! ```css
//! address, cite, dfn, em, i, var { font-style: italic; }
//! del, s, strike { text-decoration: line-through; }
//! ins, u { text-decoration: underline; }
//! code, kbd, pre, samp, tt { font-family: monospace; }
//! blockquote { margin-block: 1em; margin-inline: 40px; }
//! small { font-size: smaller; }
//! sub { vertical-align: sub; }  sup { vertical-align: super; }
//! mark { background: yellow; color: black; }
//! :link { color: #0000EE; cursor: pointer; }
//! ```
//!
//! A UA default is a DECLARED value: it beats an inherited one. Mail sets a
//! colour and a font on its wrapper (`<td style="color:#333;
//! font-family:Helvetica">`), and the link inside must still be blue and the
//! code inside still monospace. The last two tests check what is PAINTED.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::{DomId, IdOrClass, NodeData, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontFamily, StyleFontStyle},
            length::SizeMetric,
            ColorU,
        },
        property::{CssProperty, CssPropertyType},
        style::{
            background::StyleBackgroundContent, effects::StyleCursor, text::StyleTextDecoration,
            StyleVerticalAlign,
        },
    },
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const MAIL: &str = "<html><head></head><body><div>\
<p><em id=\"em\">em</em> <i id=\"i\">i</i> <cite id=\"cite\">cite</cite> \
<var id=\"var\">var</var> <dfn id=\"dfn\">dfn</dfn></p>\
<address id=\"address\">address</address>\
<p><s id=\"s\">s</s> <del id=\"del\">del</del> <strike id=\"strike\">strike</strike> \
<u id=\"u\">u</u> <ins id=\"ins\">ins</ins></p>\
<p><code id=\"code\">code</code> <kbd id=\"kbd\">kbd</kbd> <samp id=\"samp\">samp</samp> \
<tt id=\"tt\">tt</tt></p>\
<pre id=\"pre\">pre</pre>\
<blockquote id=\"blockquote\">quoted</blockquote>\
<p><small id=\"small\">small</small> x<sub id=\"sub\">2</sub> x<sup id=\"sup\">2</sup> \
<mark id=\"mark\">mark</mark> <a id=\"link\" href=\"https://example.org\">link</a></p>\
</div></body></html>";

/// The mail through the loader AzMail uses, cascaded (no window: the light
/// table).
fn styled(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("the mail parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

fn node_with_id(sd: &StyledDom, id: &str) -> NodeId {
    sd.node_data
        .as_ref()
        .iter()
        .position(|nd: &NodeData| {
            nd.get_ids_and_classes()
                .iter()
                .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == id))
        })
        .map(NodeId::new)
        .unwrap_or_else(|| panic!("no element with id {id}"))
}

/// The resolved value of `ty` on the element with `id`.
fn prop(sd: &StyledDom, id: &str, ty: CssPropertyType) -> Option<CssProperty> {
    let n = node_with_id(sd, id);
    let node_data = sd.node_data.as_container();
    let state = sd.get_styled_node_state(&n);
    sd.get_css_property_cache()
        .get_property(&node_data[n], &n, &state, &ty)
        .cloned()
}

#[test]
fn emphasis_citations_variables_definitions_and_addresses_are_italic() {
    let sd = styled(MAIL);
    for id in ["em", "i", "cite", "var", "dfn", "address"] {
        assert!(
            matches!(
                prop(&sd, id, CssPropertyType::FontStyle),
                Some(CssProperty::FontStyle(CssPropertyValue::Exact(
                    StyleFontStyle::Italic
                )))
            ),
            "<{id}> is italic: {:?}",
            prop(&sd, id, CssPropertyType::FontStyle)
        );
    }
}

#[test]
fn struck_and_inserted_text_is_lined_through_or_underlined() {
    let sd = styled(MAIL);
    for (id, want) in [
        ("s", StyleTextDecoration::LineThrough),
        ("del", StyleTextDecoration::LineThrough),
        ("strike", StyleTextDecoration::LineThrough),
        ("u", StyleTextDecoration::Underline),
        ("ins", StyleTextDecoration::Underline),
    ] {
        let got = prop(&sd, id, CssPropertyType::TextDecoration);
        assert!(
            matches!(&got, Some(CssProperty::TextDecoration(CssPropertyValue::Exact(d))) if *d == want),
            "<{id}> has text-decoration {want:?}: {got:?}"
        );
    }
}

fn is_monospace(p: &Option<CssProperty>) -> bool {
    matches!(p, Some(CssProperty::FontFamily(CssPropertyValue::Exact(families)))
        if families.as_ref().iter().any(|f| matches!(f, StyleFontFamily::System(s) if s.as_str() == "monospace")))
}

#[test]
fn code_keyboard_sample_teletype_and_preformatted_text_are_monospace() {
    let sd = styled(MAIL);
    for id in ["code", "kbd", "samp", "tt", "pre"] {
        let got = prop(&sd, id, CssPropertyType::FontFamily);
        assert!(is_monospace(&got), "<{id}> is monospace: {got:?}");
    }
}

#[test]
fn a_blockquote_is_indented_40px_and_spaced_1em() {
    let sd = styled(MAIL);
    let px = |ty| match prop(&sd, "blockquote", ty) {
        Some(CssProperty::MarginLeft(CssPropertyValue::Exact(m))) => Some(m.inner),
        Some(CssProperty::MarginRight(CssPropertyValue::Exact(m))) => Some(m.inner),
        Some(CssProperty::MarginTop(CssPropertyValue::Exact(m))) => Some(m.inner),
        Some(CssProperty::MarginBottom(CssPropertyValue::Exact(m))) => Some(m.inner),
        _ => None,
    };
    for ty in [CssPropertyType::MarginLeft, CssPropertyType::MarginRight] {
        let v = px(ty).unwrap_or_else(|| panic!("blockquote {ty:?} is set"));
        assert_eq!(v.metric, SizeMetric::Px, "{ty:?}");
        assert_eq!(v.number.get(), 40.0, "{ty:?}");
    }
    for ty in [CssPropertyType::MarginTop, CssPropertyType::MarginBottom] {
        let v = px(ty).unwrap_or_else(|| panic!("blockquote {ty:?} is set"));
        assert_eq!(v.metric, SizeMetric::Em, "{ty:?}");
        assert_eq!(v.number.get(), 1.0, "{ty:?}");
    }
}

#[test]
fn small_sub_sup_and_mark_look_as_in_a_browser() {
    let sd = styled(MAIL);
    let small = prop(&sd, "small", CssPropertyType::FontSize);
    assert!(
        matches!(&small, Some(CssProperty::FontSize(CssPropertyValue::Exact(f)))
            if f.inner.metric == SizeMetric::Em && f.inner.number.get() < 1.0),
        "<small> is smaller: {small:?}"
    );
    assert!(
        matches!(
            prop(&sd, "sub", CssPropertyType::VerticalAlign),
            Some(CssProperty::VerticalAlign(CssPropertyValue::Exact(
                StyleVerticalAlign::Sub
            )))
        ),
        "<sub> is vertical-align: sub"
    );
    assert!(
        matches!(
            prop(&sd, "sup", CssPropertyType::VerticalAlign),
            Some(CssProperty::VerticalAlign(CssPropertyValue::Exact(
                StyleVerticalAlign::Superscript
            )))
        ),
        "<sup> is vertical-align: super"
    );
    let mark = prop(&sd, "mark", CssPropertyType::BackgroundContent);
    let yellow = ColorU {
        r: 255,
        g: 255,
        b: 0,
        a: 255,
    };
    assert!(
        matches!(&mark, Some(CssProperty::BackgroundContent(CssPropertyValue::Exact(v)))
            if v.as_ref().iter().any(|b| matches!(b, StyleBackgroundContent::Color(c) if *c == yellow))),
        "<mark> has a yellow background: {mark:?}"
    );
}

#[test]
fn a_link_is_blue_and_shows_the_pointer() {
    let sd = styled(MAIL);
    let blue = ColorU {
        r: 0,
        g: 0,
        b: 0xee,
        a: 255,
    };
    let colour = prop(&sd, "link", CssPropertyType::TextColor);
    assert!(
        matches!(&colour, Some(CssProperty::TextColor(CssPropertyValue::Exact(c))) if c.inner == blue),
        "a link is #0000EE: {colour:?}"
    );
    assert!(
        matches!(
            prop(&sd, "link", CssPropertyType::Cursor),
            Some(CssProperty::Cursor(CssPropertyValue::Exact(
                StyleCursor::Pointer
            )))
        ),
        "a link shows the pointer"
    );
}

/// Every text run the markup paints: `(colour, font hash)`.
fn painted_runs(markup: &str) -> Vec<(ColorU, u64)> {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 200.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled(markup), &ws, &rr, &sc, &mut dbg)
        .expect("the mail lays out");
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text {
                color,
                font_hash,
                glyphs,
                ..
            } if !glyphs.is_empty() => Some((*color, font_hash.font_hash)),
            _ => None,
        })
        .collect()
}

/// The wrapper every mail has: a colour and a font on the container.
const STYLED_WRAPPER: &str = "<html><head></head><body>\
<div style=\"color: #333333; font-family: serif\">\
<p>Read the <a href=\"https://example.org\">report</a> and run <code>make</code>.</p>\
</div></body></html>";

#[test]
fn a_link_under_a_coloured_wrapper_paints_in_the_link_colour() {
    let runs = painted_runs(STYLED_WRAPPER);
    let grey = ColorU {
        r: 0x33,
        g: 0x33,
        b: 0x33,
        a: 255,
    };
    let blue = ColorU {
        r: 0,
        g: 0,
        b: 0xee,
        a: 255,
    };
    assert!(
        runs.iter().any(|(c, _)| *c == grey),
        "the paragraph paints in the wrapper's colour: {runs:?}"
    );
    assert!(
        runs.iter().any(|(c, _)| *c == blue),
        "the link paints #0000EE, the UA default beats the inherited colour: {runs:?}"
    );
}

#[test]
fn code_under_a_wrapper_with_a_font_paints_in_another_font() {
    let runs = painted_runs(STYLED_WRAPPER);
    let mut fonts: Vec<u64> = runs.iter().map(|(_, f)| *f).collect();
    fonts.sort_unstable();
    fonts.dedup();
    assert!(
        fonts.len() >= 2,
        "<code> is monospace although its wrapper sets `font-family: serif`: {runs:?}"
    );
}
