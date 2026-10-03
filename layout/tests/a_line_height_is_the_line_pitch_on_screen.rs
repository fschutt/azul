//! `line-height: 19px` lays the lines of a paragraph exactly 19px apart - on
//! SCREEN (the window layout), as `an_absolute_line_height_is_the_exact_line_pitch`
//! pins it on paper.
//!
//! The ledger's "19px pitches 19.55px" (pdfocr, 2026-10-02, against azul at
//! 2e92c759b) was the strut taking a synthetic 0.8em / 0.2em ascent / descent
//! while the glyphs took their face's (TEXTENG, wave 5); wave 6 then rounded
//! both to whole pixels with one leading split (MAILENG6). Any face whose
//! glyph box and strut disagree (a different ascent split, the macOS +15%
//! Times / Helvetica / Courier ascent on one and not the other) makes the
//! union of the two taller than the line-height. Five lines, every family the
//! pdfocr markup names, the pdfocr region itself.

use azul_layout::window::LayoutWindow;

use crate::table_markup::{glyph_runs, laid_out, prose};

/// The distinct baselines of every glyph, top to bottom.
fn baselines(lw: &LayoutWindow) -> Vec<f32> {
    let mut ys: Vec<f32> = glyph_runs(lw)
        .into_iter()
        .flatten()
        .map(|(_, y)| y)
        .collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    ys
}

/// Five lines or more, each `px` below the one before, to the hundredth.
fn assert_pitch(lw: &LayoutWindow, what: &str, px: f32) {
    let ys = baselines(lw);
    assert!(
        ys.len() >= 5,
        "premise, five lines or more ({what}): {ys:?}"
    );
    for pair in ys.windows(2) {
        let pitch = pair[1] - pair[0];
        assert!(
            (pitch - px).abs() < 0.02,
            "{what}: the lines are {pitch}px apart, not {px}px: {ys:?}"
        );
    }
}

const FAMILIES: [&str; 4] = [
    "",
    "Helvetica, Arial, sans-serif",
    "Times, 'Times New Roman', serif",
    "'Courier New', Courier, monospace",
];

#[test]
fn a_19px_line_height_lays_five_lines_19px_apart() {
    for family in FAMILIES {
        for size in ["11pt", "16px"] {
            let family_css = if family.is_empty() {
                String::new()
            } else {
                format!("font-family: {family};")
            };
            let lw = laid_out(
                &format!(
                    "<html><head><style>* {{ margin: 0; padding: 0; }} p {{ width: 300px; \
                     font-size: {size}; line-height: 19px; {family_css} }}</style></head>\
                     <body><p>{}</p></body></html>",
                    prose(80)
                ),
                800.0,
                600.0,
            );
            assert_pitch(
                &lw,
                &format!("{family:?} at {size}, line-height 19px"),
                19.0,
            );
        }
    }
}

#[test]
fn the_pdfocr_region_keeps_its_line_height_as_the_pitch() {
    // pdfocr's html2pdf markup (results/azul-text-indent-repro): a justified,
    // absolutely positioned 12pt Times region with `line-height: 16pt`
    // (21.33px) around an indented paragraph.
    let lw = laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; box-sizing: border-box; }} \
             .page {{ position: relative; width: 600pt; height: 400pt; overflow: hidden; }} \
             .region {{ position: absolute; color: #000; font-family: Times, 'Times New Roman', \
             serif; }}</style></head><body><div class=\"page\"><div class=\"region\" \
             style=\"left: 10%; top: 10%; width: 50%; font-size: 12pt; line-height: 16pt; \
             text-align: justify;\"><p style=\"text-indent: 40pt;\">{}</p></div></div>\
             </body></html>",
            prose(80)
        ),
        1000.0,
        800.0,
    );
    assert_pitch(
        &lw,
        "the pdfocr region, line-height 16pt",
        16.0 * 96.0 / 72.0,
    );
}
