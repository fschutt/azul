//! `line-height` in `rem` resolves against the ROOT element's font size, in
//! `vw` / `vh` against the window - on screen, and again after the window is
//! resized (CSS Values 4 s6.1.1 / s6.1.2; CSS Inline 3 s4.2: a length
//! computes to an absolute length, which is what descendants inherit).
//!
//! The ledger's "line-height rem / vw / vh is rejected" predates TEXTENG's
//! `StyleLineHeight` enum (wave 5), pinned on paper at the UA's 16px root and
//! a fixed page (`a_line_height_in_em_or_percent_inherits_as_a_length`). These
//! pin the rest: a root that is not 16px, a descendant with its own font size,
//! the window layout, and a resize that keeps the window's caches.

use azul_layout::window::LayoutWindow;

use crate::table_markup::{glyph_runs, laid_out, lay_out_in, prose};

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

/// Three lines or more, each `px` below the one before, to the hundredth.
fn assert_pitch(lw: &LayoutWindow, what: &str, px: f32) {
    let ys = baselines(lw);
    assert!(
        ys.len() >= 3,
        "premise, three lines or more ({what}): {ys:?}"
    );
    for pair in ys.windows(2) {
        let pitch = pair[1] - pair[0];
        assert!(
            (pitch - px).abs() < 0.02,
            "{what}: the lines are {pitch}px apart, not {px}px: {ys:?}"
        );
    }
}

/// A page whose root is `root_font_size` and whose 300px wide paragraph,
/// at `p_font_size`, sits in a container declaring `line_height`.
fn page(root_font_size: &str, line_height: &str, p_font_size: &str) -> String {
    format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} html {{ font-size: \
         {root_font_size}; }} div {{ width: 300px; font-size: 12px; line-height: \
         {line_height}; }} p {{ font-size: {p_font_size}; }}</style></head><body><div><p>{}\
         </p></div></body></html>",
        prose(60)
    )
}

#[test]
fn a_line_height_in_rem_follows_the_root_font_size() {
    // 2rem of a 20px root, whatever the container's 12px or the paragraph's 18px.
    let lw = laid_out(&page("20px", "2rem", "18px"), 800.0, 600.0);
    assert_pitch(&lw, "line-height: 2rem under a 20px root", 40.0);
}

#[test]
fn a_line_height_in_viewport_units_follows_the_window() {
    for (value, px) in [
        ("5vh", 30.0),
        ("4vw", 32.0),
        ("5vmin", 30.0),
        ("4vmax", 32.0),
    ] {
        let lw = laid_out(&page("16px", value, "18px"), 800.0, 600.0);
        assert_pitch(
            &lw,
            &format!("line-height: {value} in an 800 x 600 window"),
            px,
        );
    }
}

#[test]
fn a_line_height_in_vh_follows_a_resized_window() {
    let markup = page("16px", "5vh", "18px");
    let mut lw = laid_out(&markup, 800.0, 600.0);
    assert_pitch(
        &lw,
        "premise, line-height: 5vh in an 800 x 600 window",
        30.0,
    );
    // The same window, now 800 x 800: its caches survive the resize.
    lay_out_in(&mut lw, &markup, 800.0, 800.0);
    assert_pitch(&lw, "line-height: 5vh after a resize to 800 x 800", 40.0);
}
