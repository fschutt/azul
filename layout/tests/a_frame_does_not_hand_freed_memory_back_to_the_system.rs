//! A frame does not hand freed memory back to the system.
//!
//! Returning freed pages (`malloc_zone_pressure_relief` on macOS,
//! `malloc_trim` on Linux) walks the allocator's free lists and unmaps
//! pages the next frame faults in again: a tenth of each animation frame of
//! AzDrive's window when every layout pass did it. A layout pass only notes
//! that memory was freed; the window hands it back once it goes idle
//! (`probe::return_freed_memory`).

use azul_core::{dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, probe, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn page(rows: usize) -> Dom {
    let mut body = Dom::create_body().with_css("margin: 0;");
    for i in 0..rows {
        body = body.with_child(Dom::create_p_with_text(format!("row {i}")));
    }
    body
}

fn lay_out(lw: &mut LayoutWindow, ws: &FullWindowState, rows: usize) {
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page(rows)),
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("laid out");
}

#[test]
fn a_layout_pass_returns_no_memory_and_the_idle_window_returns_it_once() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lay_out(&mut lw, &ws, 200);

    let before = probe::allocator_purges_on_this_thread();
    lay_out(&mut lw, &ws, 20);
    lay_out(&mut lw, &ws, 200);
    assert_eq!(
        probe::allocator_purges_on_this_thread() - before,
        0,
        "a layout pass handed freed memory back to the system"
    );
    probe::return_freed_memory();
    assert_eq!(
        probe::allocator_purges_on_this_thread() - before,
        1,
        "the idle window hands back what the passes freed"
    );
    probe::return_freed_memory();
    assert_eq!(
        probe::allocator_purges_on_this_thread() - before,
        1,
        "nothing more was freed since"
    );
}
