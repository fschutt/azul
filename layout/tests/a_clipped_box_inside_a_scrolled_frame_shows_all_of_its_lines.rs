//! A clipped box inside a scrolled frame shows all of its lines - its layer
//! is clipped where its own clip IS, not where an outer clip's numbers say.
//!
//! A box with `overflow: hidden` is a scroll frame, which the CPU compositor
//! makes a layer, clipped at composite time by the `PushClip`s open around
//! it (`Layer::static_clip`). Those were intersected as they stood in the
//! list - but a clip opened OUTSIDE the enclosing scroll frame is in the
//! window's space and one opened inside it in the frame's CONTENT space. A
//! box down a scrolled frame got the intersection of the two: AzCalendar's
//! week, scrolled to the working day, cut "Lunch with Ana"'s time line after
//! a few pixels and showed the other events' blocks without any text
//! (PIM6, 2026-10-03).

use std::collections::HashMap;

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, cpurender, glyph_cache::GlyphCache,
    solver3::display_list::DisplayListItem, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: u32 = 220;
const H: u32 = 220;

/// A 200 x 200 scrolling list: `above` px of space, then a 48 px clipped
/// block of two lines, then more space. Rendered with the list scrolled by
/// `scroll` through the layered compositor.
fn rendered(above: f32, scroll: f32) -> cpurender::AzulPixmap {
    let line = "white-space: nowrap; overflow: hidden; flex-shrink: 0;";
    let page = Dom::create_body()
        .with_css("margin: 0; padding: 0; background: #ffffff;")
        .with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 200px; overflow-y: auto; background: #ffffff;")
                .with_child(Dom::create_div().with_css(&format!("height: {above}px;")))
                .with_child(
                    Dom::create_div()
                        .with_css(
                            "height: 48px; overflow: hidden; display: flex; flex-direction: \
                             column; padding: 3px 6px; font-size: 16px; color: #000000; \
                             background: #ffffff;",
                        )
                        .with_child(Dom::create_div().with_css(line).with_child(
                            Dom::create_text_do_not_use_without_block_level_wrapper(
                                "Lunch with Ana",
                            ),
                        ))
                        .with_child(Dom::create_div().with_css(line).with_child(
                            Dom::create_text_do_not_use_without_block_level_wrapper(
                                "12:30 - 13:30",
                            ),
                        )),
                )
                .with_child(Dom::create_div().with_css("height: 600px;")),
        );
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W as f32, H as f32);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &rr,
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    let dl = &lw.layout_results[&DomId::ROOT_ID].display_list;

    // The list's frame: the scroll frame 200 px tall.
    let mut offsets = cpurender::ScrollOffsetMap::new();
    let list = dl.items.iter().find_map(|item| match item {
        DisplayListItem::PushScrollFrame {
            clip_bounds,
            scroll_id,
            ..
        } if (clip_bounds.inner().size.height - 200.0).abs() < 0.5 => Some(*scroll_id),
        _ => None,
    });
    let list = list.expect("harness: the list is a scroll frame");
    offsets.insert(list, (0.0, scroll));

    let mut glyph_cache = GlyphCache::new();
    let render_state = cpurender::CpuRenderState::new(offsets);
    let mut compositor = cpurender::CompositorState::new(W, H);
    compositor.allocate_layers_from_display_list(dl, 1.0, &HashMap::new(), &HashMap::new());
    compositor
        .render_layers(
            dl,
            1.0,
            &rr,
            &lw.font_manager,
            &mut glyph_cache,
            &render_state,
        )
        .expect("the layers render");
    let mut out = cpurender::AzulPixmap::new(W, H).expect("a pixmap");
    out.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut out, 1.0);
    out
}

/// Dark pixels in rows `y0..y1` of the list.
fn ink(p: &cpurender::AzulPixmap, y0: u32, y1: u32) -> usize {
    let d = p.data();
    (y0..y1)
        .flat_map(|y| (0..200).map(move |x| ((y * W + x) * 4) as usize))
        .filter(|&i| d[i] < 128 && d[i + 1] < 128 && d[i + 2] < 128)
        .count()
}

#[test]
fn a_clipped_box_down_a_scrolled_frame_shows_both_of_its_lines() {
    // Unscrolled, the block is at 50..98 of the window: the reference.
    let reference = rendered(50.0, 0.0);
    // Scrolled by 250, the block 300 px down the list lands at 50..98 too.
    let scrolled = rendered(300.0, 250.0);
    let (first_ref, second_ref) = (ink(&reference, 50, 72), ink(&reference, 72, 98));
    assert!(
        first_ref > 30 && second_ref > 30,
        "harness: both lines are painted unscrolled ({first_ref}, {second_ref} dark pixels)"
    );
    let (first, second) = (ink(&scrolled, 50, 72), ink(&scrolled, 72, 98));
    assert!(
        first * 10 >= first_ref * 9,
        "the first line shows down the scrolled list: {first} dark pixels, {first_ref} unscrolled"
    );
    assert!(
        second * 10 >= second_ref * 9,
        "the second line shows down the scrolled list: {second} dark pixels, {second_ref} \
         unscrolled"
    );
}
