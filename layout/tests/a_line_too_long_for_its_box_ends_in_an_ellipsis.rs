//! A line too long for its box ends in an ellipsis: `text-overflow: ellipsis`.
//!
//! CSS Overflow 3 §3.1: on a block container that clips its inline axis
//! (`overflow` other than `visible`), `text-overflow: ellipsis` hides the
//! characters at the end edge of a line that runs past it and draws an
//! ellipsis (U+2026, or three dots where the block's font has none) right
//! after the characters that stay. The engine parsed the property and never
//! painted it: a status bar's or a file list's text was cut mid-letter at the
//! box edge (AzMail's status line, AzDrive's names, AzContacts' cards).
//!
//! The ellipsis is paint: the clipped line keeps its glyphs where `clip` puts
//! them, and what the ellipsis replaces is gone from the picture. A line that
//! fits, and a box whose inline axis does not clip, paint as they do with
//! `clip`. Every line is cut on its own.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const LONG: &str = "The quick brown fox jumps over the lazy dog";
const BOX_WIDTH: f32 = 120.0;

/// One painted glyph: its glyph id and where its pen starts (x, baseline y).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Painted {
    id: u32,
    x: f32,
    y: f32,
}

/// The glyphs painted for `text` in a `<p>` `BOX_WIDTH` wide with `css`, in
/// list order.
fn painted(text: &str, css: &str) -> Vec<Painted> {
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_p()
            .with_css(&format!(
                "margin: 0px; width: {BOX_WIDTH}px; font-size: 14px; {css}"
            ))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
    );
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .clone();
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => Some(glyphs.iter().map(|g| Painted {
                id: g.index,
                x: g.point.x,
                y: g.point.y,
            })),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The glyphs of `ellipsed` past the run it shares with `clipped`, and how
/// long that run is: what the ellipsis painted, and where.
fn ellipsis_of(clipped: &[Painted], ellipsed: &[Painted]) -> (usize, Vec<Painted>) {
    let kept = clipped
        .iter()
        .zip(ellipsed)
        .take_while(|(c, e)| c.id == e.id && (c.x - e.x).abs() < 0.01 && (c.y - e.y).abs() < 0.01)
        .count();
    (kept, ellipsed[kept..].to_vec())
}

fn assert_is_an_ellipsis(extra: &[Painted], what: &str) {
    assert!(
        extra.len() == 1 || (extra.len() == 3 && extra.iter().all(|g| g.id == extra[0].id)),
        "{what}: one ellipsis glyph (or three dots) ends the line, painted {extra:?}"
    );
    assert!(extra.iter().all(|g| g.id != 0), "{what}: not .notdef: {extra:?}");
}

#[test]
fn a_line_too_long_for_its_box_ends_in_an_ellipsis() {
    let line = "white-space: nowrap; overflow: hidden;";
    let clipped = painted(LONG, &format!("{line} text-overflow: clip;"));
    let ellipsed = painted(LONG, &format!("{line} text-overflow: ellipsis;"));
    assert!(
        clipped.iter().any(|g| g.x >= BOX_WIDTH),
        "the line runs past its box: {clipped:?}"
    );

    let (kept, extra) = ellipsis_of(&clipped, &ellipsed);
    assert!(kept > 0, "the first characters stay: {ellipsed:?}");
    assert!(
        kept < clipped.len() && ellipsed.len() < clipped.len(),
        "the characters at the end edge are hidden: {} painted, {} with clip",
        ellipsed.len(),
        clipped.len()
    );
    assert_is_an_ellipsis(&extra, "one line");
    let last_kept = ellipsed[kept - 1];
    for g in &extra {
        assert!(
            g.x > last_kept.x && g.x < BOX_WIDTH - 4.0,
            "the ellipsis comes right after the last kept character, inside the box: {g:?} \
             after {last_kept:?}"
        );
        assert!((g.y - last_kept.y).abs() < 0.5, "on the line's baseline: {g:?}");
    }
    assert!(
        ellipsed[..kept].iter().all(|g| g.x < BOX_WIDTH),
        "nothing kept starts past the box"
    );
}

#[test]
fn a_line_that_fits_or_a_box_that_does_not_clip_is_painted_as_with_clip() {
    let line = "white-space: nowrap; overflow: hidden;";
    assert_eq!(
        painted("Short", &format!("{line} text-overflow: ellipsis;")),
        painted("Short", &format!("{line} text-overflow: clip;")),
        "a line that fits keeps every character"
    );
    assert_eq!(
        painted(LONG, "white-space: nowrap; overflow: visible; text-overflow: ellipsis;"),
        painted(LONG, "white-space: nowrap; overflow: visible; text-overflow: clip;"),
        "a box that does not clip its inline axis has no end edge to ellipsize at"
    );
}

#[test]
fn every_line_too_long_for_its_box_ends_in_its_own_ellipsis() {
    let two = format!("{LONG}\n{LONG}");
    let line = "white-space: pre; overflow: hidden;";
    let clipped = painted(&two, &format!("{line} text-overflow: clip;"));
    let ellipsed = painted(&two, &format!("{line} text-overflow: ellipsis;"));

    let mut baselines: Vec<f32> = clipped.iter().map(|g| g.y).collect();
    baselines.sort_by(f32::total_cmp);
    baselines.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    assert_eq!(baselines.len(), 2, "two lines: {clipped:?}");

    for (n, baseline) in baselines.iter().enumerate() {
        let on = |gs: &[Painted]| -> Vec<Painted> {
            gs.iter().copied().filter(|g| (g.y - baseline).abs() < 0.5).collect()
        };
        let (c, e) = (on(&clipped), on(&ellipsed));
        let (kept, extra) = ellipsis_of(&c, &e);
        assert!(kept > 0 && kept < c.len(), "line {n} is cut: {e:?}");
        assert_is_an_ellipsis(&extra, &format!("line {n}"));
    }
}
