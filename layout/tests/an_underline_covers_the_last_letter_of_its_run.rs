//! An underline covers the last letter of its run.
//!
//! Found while probing the AzMail samples (mail links are underlined): the
//! underline of `<a>link</a>` stopped under "lin", and `<u>W</u>` had none.
//! The display list measured a glyph run from its first glyph's pen to its
//! LAST glyph's pen - without that glyph's advance, which a paint run does
//! not carry. Three passes shared the mistake: the run's inline background
//! and border (a one-letter `<mark>` got none), its text decorations, and
//! the hit-test area of its text (a one-letter link could not be hit - the
//! area of a one-glyph run is empty and was skipped).
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const FONT: f32 = 20.0;

const MAIL: &str = "<html><head></head><body>\
<p style=\"font-size: 20px;\"><u>W</u> and <u>underlined</u> text</p>\
</body></html>";

struct Painted {
    /// `(x, width)` of every underline.
    underlines: Vec<(f32, f32)>,
    /// Every text run's glyph pens.
    runs: Vec<Vec<f32>>,
}

fn painted() -> Painted {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
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
    let mut underlines = Vec::new();
    let mut runs = Vec::new();
    for item in &result.display_list.items {
        match item {
            DisplayListItem::Underline { bounds, .. } => {
                let b = bounds.inner();
                underlines.push((b.origin.x, b.size.width));
            }
            DisplayListItem::Text { glyphs, .. } if !glyphs.is_empty() => {
                runs.push(glyphs.iter().map(|g| g.point.x).collect());
            }
            _ => {}
        }
    }
    Painted { underlines, runs }
}

#[test]
fn a_one_letter_underline_is_as_wide_as_its_letter() {
    let p = painted();
    assert_eq!(p.underlines.len(), 2, "two <u> runs: {:?}", p.underlines);
    for (x, w) in &p.underlines {
        assert!(
            *w >= 0.4 * FONT,
            "every underline spans at least one letter (W is ~0.9em): {w} at x={x}"
        );
    }
}

#[test]
fn an_underline_reaches_past_the_pen_of_its_last_letter() {
    let p = painted();
    // "underlined" is the only run of ten glyphs.
    let word = p
        .runs
        .iter()
        .find(|r| r.len() == 10)
        .unwrap_or_else(|| panic!("the run \"underlined\": {:?}", p.runs));
    let last_pen = word.iter().copied().fold(f32::MIN, f32::max);
    let (x, w) = p
        .underlines
        .iter()
        .copied()
        .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    assert!(
        x + w >= last_pen + 0.3 * FONT,
        "the underline ends after the last letter (\"d\" is ~0.5em wide): ends at {} but the \
         last pen is at {last_pen}",
        x + w
    );
}
