//! A measured DOM is as big as what shows of it: a box that scrolls or clips keeps
//! its content inside its own box on the axes it clips, so a content-sized window
//! or view around a list in a 100 px scroll box is that box's size, not its rows'.
//! AzDrive's country picker (241 countries in a 260 px scroll box) opened its
//! dialog window 4218 px tall.

use azul_core::{dom::Dom, geom::LogicalSize};
use azul_layout::window::LayoutWindow;
use rust_fontconfig::FcFontCache;

/// Fifty 20 px rows in a column box with `css`, in a 200 px wide column.
fn rows(css: &str) -> Dom {
    let mut list = Dom::create_div().with_css(css);
    for _ in 0..50 {
        list = list.with_child(Dom::create_div().with_css("height: 20px; flex-shrink: 0;"));
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 200px;")
        .with_child(list)
}

const SCROLLS: &str = "display: flex; flex-direction: column; max-height: 100px; overflow-y: auto;";
const SPILLS: &str = "display: flex; flex-direction: column; max-height: 100px;";

#[test]
fn a_scroll_box_keeps_its_rows_out_of_the_measured_size() {
    let lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let available = LogicalSize::new(800.0, 100_000.0);

    let extent = lw.measure_dom(rows(SCROLLS), available);
    assert!((extent.height - 100.0).abs() < 1.0, "the scroll box's 100 px: {extent:?}");
    let fit = lw.measure_dom_shrink_to_fit(rows(SCROLLS), available);
    assert!((fit.height - 100.0).abs() < 1.0, "shrink-to-fit too: {fit:?}");
    assert!((fit.width - 200.0).abs() < 1.0, "the column's width: {fit:?}");

    // Control: rows that spill out of a box that does not clip are on screen and count.
    let spilled = lw.measure_dom(rows(SPILLS), available);
    assert!(spilled.height > 999.0, "the visible overflow counts: {spilled:?}");
}
