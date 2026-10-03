//! A long scrolling list of rows - an Avatar (a clipped circle with the initials) and a name -
//! lays out in time linear in its rows, and an avatar that clips its initials costs about what a
//! circle that does not clip them costs.
//!
//! Found by PIMDRIVE7 (2026-10-03) on AzContacts' list pane: with the 300 contacts of
//! `--sample` (2653 nodes) the first layout took 2 s (`after_layout_and_dl 2021 ms`) and the
//! process grew past 1.5 GB DURING `layout_new_generation` (the capped runner killed it); 100
//! contacts took 0.67 s and 534 MB, 200 contacts 739 MB, one contact 347 MB. The rows are plain:
//! a flex row, the azul `Avatar` (Small: 24 px, `border-radius: 50%`, `overflow: hidden`, the
//! initials centred), a column with the name. Owner: LAYOUT7 / PAINT7 (solver3, the display
//! list); the app does nothing unusual.
//!
//! AzTasks' list of 300 plain rows (3372 nodes) took 1.2 s and 654 MB in the same build: the
//! avatars' rows cost about twice as much per node and several times the memory. Suspect: each
//! `overflow: hidden` + `border-radius` box makes something the size of the window (a clip
//! mask?) - 300 of them at ~3 MB each is the gigabyte. The tests: four times the rows may take
//! about four times as long (a quadratic pass takes sixteen; the bound allows eight), and the
//! avatar rows may take at most twice as long as the same rows with an unclipped circle.

use std::time::Instant;

use azul_core::{dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::avatar::{Avatar, AvatarSize},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The circle a row starts with: azul's Avatar (it clips its content to the circle), or - the
/// control - a plain circle of the same size with the initials that clips nothing.
fn circle(i: usize, clipped: bool) -> Dom {
    let initials = AzString::from(format!("P{}", i % 10));
    if clipped {
        return Avatar::create(initials).with_size(AvatarSize::Small).dom();
    }
    Dom::create_div()
        .with_css(
            "display: flex; align-items: center; justify-content: center; width: 24px; height: \
             24px; border-radius: 12px; flex-shrink: 0; font-size: 11px; background: #cccccc;",
        )
        .with_child(Dom::create_span_with_text(initials))
}

/// AzContacts' list pane with `rows` contacts: the scrolling list of rows beside the A-Z bar,
/// under a header.
fn list_pane(rows: usize, clipped: bool) -> Dom {
    let mut list = Dom::create_div().with_css(
        "display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;",
    );
    for i in 0..rows {
        let name = Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; flex-grow: 1; padding-left: 8px; \
                 min-width: 0px;",
            )
            .with_child(
                Dom::create_div()
                    .with_css("font-size: 13px;")
                    .with_child(Dom::create_span_with_text(AzString::from(format!(
                        "Person {i:03}"
                    )))),
            );
        list.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 4px 8px;",
                )
                .with_child(circle(i, clipped))
                .with_child(name),
        );
    }
    let mut jump = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 18px; flex-shrink: 0; font-size: 10px;",
    );
    for letter in 'A'..='Z' {
        jump.add_child(
            Dom::create_div()
                .with_child(Dom::create_span_with_text(AzString::from(letter.to_string()))),
        );
    }
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; height: 100%; margin: 0px;")
            .with_child(Dom::create_div().with_css("height: 28px; flex-shrink: 0;"))
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;",
                    )
                    .with_children(vec![list, jump].into()),
            ),
    )
}

/// How long the first layout (and display list) of `rows` rows takes, in seconds.
fn first_layout_secs(fonts: &FcFontCache, rows: usize, clipped: bool) -> f64 {
    let mut dom = list_pane(rows, clipped);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(fonts.clone()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1100.0, 720.0);
    lw.current_window_state = ws.clone();
    let start = Instant::now();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    start.elapsed().as_secs_f64()
}

#[test]
fn a_long_list_of_avatar_rows_lays_out_in_linear_time() {
    let fonts = FcFontCache::build();
    // Warm up (font loading and the first shaping of the glyphs) so the runs compare layout.
    first_layout_secs(&fonts, 10, true);
    let short = first_layout_secs(&fonts, 60, true).max(1e-4);
    let long = first_layout_secs(&fonts, 240, true);
    assert!(
        long < short * 8.0,
        "240 rows took {long:.3} s, 60 rows {short:.3} s: {:.1} times as long for 4 times the rows",
        long / short
    );
}

#[test]
fn an_avatar_that_clips_its_initials_costs_about_what_an_unclipped_circle_costs() {
    let fonts = FcFontCache::build();
    first_layout_secs(&fonts, 10, false);
    let plain = first_layout_secs(&fonts, 200, false).max(1e-4);
    let clipped = first_layout_secs(&fonts, 200, true);
    assert!(
        clipped < plain * 2.0,
        "200 avatar rows took {clipped:.3} s, 200 rows with a plain circle {plain:.3} s ({:.1} \
         times)",
        clipped / plain
    );
}
