//! A widget page rebuilt unchanged costs less than laying it out in a fresh
//! window.
//!
//! An app that answers a click with `RefreshDom` hands the window a new tree
//! that is mostly the old one. The rebuild reconciles the two (every matched
//! node's `animation` read from the old cascade, its properties compared
//! only where its cascade inputs moved) and lays out from the retained
//! tree; a fresh window cascades and lays out everything from nothing. When
//! the rebuild costs as much as the fresh layout, the reconcile is doing
//! the whole page's work again on every click, which is what made
//! AzWidgets' rebuild take a quarter of a second.
//!
//! Release builds only; the timings are printed for comparison. Both sides
//! are timed in the CPU time of the thread that runs them
//! (`thread_cpu_clock`), so the suite's other tests and the machine's other
//! work weigh on neither.
//!
//! Not compiled by the author (house rule).

use std::time::Duration;

use azul_core::{dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        button::{Button, ButtonType},
        tabs::{TabHeader, TabStyle},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A gallery row: a label, a tab bar and a row of buttons of every type,
/// in `theme`.
fn row(i: usize, theme: UiTheme) -> Dom {
    let labels = StringVec::from_vec(vec![
        AzString::from("General"),
        AzString::from("Details"),
        AzString::from("Sharing"),
    ]);
    let mut buttons = Dom::create_div().with_css("display: flex; flex-direction: row; gap: 6px;");
    for ty in [
        ButtonType::Default,
        ButtonType::Primary,
        ButtonType::Danger,
        ButtonType::Link,
    ] {
        buttons = buttons.with_child(
            Button::with_type(AzString::from(format!("Action {i}")), ty)
                .with_theme(theme)
                .dom(),
        );
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; padding: 8px;")
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(format!("Row {i}")),
        ))
        .with_child(
            TabHeader::create(labels)
                .with_active_tab(i % 3)
                .with_theme(theme)
                .with_style(if i.is_multiple_of(2) { TabStyle::Aurora } else { TabStyle::Rectangular })
                .dom(),
        )
        .with_child(buttons)
}

fn page() -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; overflow-y: auto; height: 100%;");
    for i in 0..60 {
        column = column.with_child(row(i, if i % 2 == 0 { UiTheme::Flat } else { UiTheme::Flora }));
    }
    Dom::create_body().with_css("margin: 0px; height: 100%;").with_child(column)
}

fn lay_out(lw: &mut LayoutWindow, mut dom: Dom) -> Duration {
    let started = crate::thread_cpu_clock::CpuInstant::now();
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    started.elapsed()
}

fn fresh_window(fonts: &FcFontCache) -> LayoutWindow {
    let mut lw = LayoutWindow::new(fonts.clone()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(900.0, 700.0);
    lw.current_window_state = ws;
    lw
}

#[test]
fn a_widget_page_rebuilt_unchanged_costs_less_than_a_fresh_layout() {
    if cfg!(debug_assertions) {
        eprintln!("[a_widget_page_rebuilt] debug build: the comparison is for release builds");
        return;
    }
    let fonts = FcFontCache::build();
    // Warm the fonts once, so neither side pays for loading them.
    let _ = lay_out(&mut fresh_window(&fonts), page());

    // The best of five of each, taken in turns and in CPU time: the pass is
    // measured, not the machine's other work, and what load remains (caches
    // shared with the rest of the suite) weighs on both sides alike.
    let mut lw = fresh_window(&fonts);
    let _ = lay_out(&mut lw, page());
    let (fresh, rebuilt) = crate::thread_cpu_clock::on_a_performance_core(|| {
        let (mut fresh, mut rebuilt) = (Duration::MAX, Duration::MAX);
        for _ in 0..5 {
            fresh = fresh.min(lay_out(&mut fresh_window(&fonts), page()));
            rebuilt = rebuilt.min(lay_out(&mut lw, page()));
        }
        (fresh, rebuilt)
    });
    eprintln!("[a_widget_page_rebuilt] fresh layout {fresh:?}, unchanged rebuild {rebuilt:?}");
    assert!(
        rebuilt < fresh,
        "rebuilding the unchanged page took {rebuilt:?}, a fresh window lays it out in {fresh:?}: \
         the rebuild redoes the whole page's work"
    );
}
