//! The size a popup's content is measured at (`LayoutWindow::measure_styled_dom_shrink_to_fit`) is the size its content takes
//! when it is laid out: a panel with a frame - the padding and the border of a dialog - is held WHOLE by the box measured for
//! it. On a phone the popover of the Dialog widget came up 12 px short (its bottom padding, the border with it): the overlay is
//! laid out at the measured size, so the panel's last 12 px were clipped.
//!
//! Each row is one shape of the dialog's panel, measured as the root of a window of its own (a block that scrolls on its y axis,
//! as the Dom pass makes an anchored popup); the first rows are the panel with its parts taken away, to say which part it is.
#![cfg(feature = "widgets")]

use azul_core::{dom::Dom, geom::LogicalSize, styled_dom::StyledDom};
use azul_layout::window::LayoutWindow;
use rust_fontconfig::FcFontCache;

/// The measured height of `root_style` > `panel_style` > `children`.
fn measured(lw: &LayoutWindow, root_style: &str, panel_style: &str, children: Vec<Dom>) -> f32 {
    let panel = children
        .into_iter()
        .fold(Dom::create_div().with_css(panel_style), Dom::with_child);
    let mut dom = Dom::create_div()
        .with_css(&format!("margin: 0; {root_style}"))
        .with_child(panel);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    lw.measure_styled_dom_shrink_to_fit(&styled, LogicalSize::new(400.0, 800.0))
        .expect("the content is measured")
        .height
}

fn block(height: f32) -> Dom {
    Dom::create_div().with_css(&format!("height: {height}px; width: 100px;"))
}

#[test]
fn a_padded_flex_panel_in_a_scrolling_root_is_measured_whole() {
    let lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let scrolls = "overflow-y: auto; overflow-x: hidden;";
    let frame = "padding: 0px 14px 12px 14px; border: 1px solid #888888;";
    let flex = "display: flex; flex-direction: column;";
    let dialog = "position: relative; flex-grow: 0;";
    let band = || {
        Dom::create_div().with_css("height: 30px; margin: 0px -14px 12px -14px; width: 100px;")
    };
    let close = || {
        Dom::create_div().with_css("position: absolute; top: 3px; right: 8px; min-width: 44px; min-height: 44px;")
    };

    // (what the panel is, what it measured, what it must measure)
    let rows = [
        ("block panel, one child", measured(&lw, scrolls, frame, vec![block(20.0)]), 34.0),
        ("flex column panel, one child", measured(&lw, scrolls, &format!("{frame} {flex}"), vec![block(20.0)]), 34.0),
        (
            "flex column, relative, no grow",
            measured(&lw, scrolls, &format!("{frame} {flex} {dialog}"), vec![block(20.0)]),
            34.0,
        ),
        (
            "the dialog: band, content",
            measured(&lw, scrolls, &format!("{frame} {flex} {dialog}"), vec![band(), block(20.0)]),
            76.0,
        ),
        (
            "the dialog: band, content, close",
            measured(&lw, scrolls, &format!("{frame} {flex} {dialog}"), vec![band(), block(20.0), close()]),
            76.0,
        ),
        (
            "the dialog in a root that does not scroll",
            measured(&lw, "", &format!("{frame} {flex} {dialog}"), vec![band(), block(20.0)]),
            76.0,
        ),
    ];
    let wrong: Vec<String> = rows
        .iter()
        .filter(|(_, got, want)| (got - want).abs() > 0.5)
        .map(|(what, got, want)| format!("{what}: measured {got}, it is {want} tall"))
        .collect();
    assert!(wrong.is_empty(), "measured short:\n{}", wrong.join("\n"));
}
