//! A change inside a CSS-transformed box is damaged where the box is painted.
//!
//! A `transform` makes the box a reference frame of the display list, and the
//! CPU rasteriser draws everything inside through the frame's matrix. The
//! damage diff walked the items in their untransformed coordinates, so a
//! change inside such a box repainted the rect the box would have WITHOUT the
//! transform. AzDrive's toasts sit in a column placed with `left: 50%;
//! transform: translateX(-50%)`: a toast that changed was repainted half a
//! card to the right of itself, and its left half kept the old pixels - the
//! toast "split in the middle".
//!
//! The check: render both frames whole, and every pixel that differs must lie
//! inside the damage.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions, ScrollOffsetMap},
    glyph_cache::GlyphCache,
    solver3::display_list::DisplayList,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 800.0;
const H: f32 = 300.0;

fn page(colour: &str, transform: &str) -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body().with_css("margin: 0; width: 800px; height: 300px;").with_child(
            Dom::create_div().with_css(&format!(
                "position: absolute; left: 50%; top: 100px; width: 200px; height: 40px; \
                 transform: {transform}; background: {colour};"
            )),
        ),
    )
}

fn laid_out(styled: StyledDom) -> (LayoutWindow, std::sync::Arc<DisplayList>) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    let dl = lw.layout_results[&DomId::ROOT_ID].display_list.clone();
    (lw, dl)
}

fn rendered(lw: &LayoutWindow, dl: &DisplayList) -> (Vec<u8>, usize) {
    let mut gc = GlyphCache::new();
    let pm = cpurender::render_with_font_manager(
        dl,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: W,
            height: H,
            dpi_factor: 1.0,
        },
        &mut gc,
    )
    .expect("the page renders");
    (pm.data().to_vec(), pm.width() as usize)
}

/// The pixels that differ between the two frames and lie outside `damage`.
fn changed_outside_damage(transform: &str) -> (usize, usize, Vec<azul_core::geom::LogicalRect>) {
    let (lw_old, old) = laid_out(page("#ff0000", transform));
    let (lw_new, new) = laid_out(page("#0000ff", transform));
    let none = ScrollOffsetMap::default();
    let damage = cpurender::compute_display_list_damage(&old, &new, &none, &none)
        .expect("the same structure: an item diff");
    let (a, w) = rendered(&lw_old, &old);
    let (b, _) = rendered(&lw_new, &new);
    let (mut changed, mut outside) = (0, 0);
    for (i, (pa, pb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        if pa == pb {
            continue;
        }
        changed += 1;
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
        let inside = damage.iter().any(|r| {
            r.origin.x <= x
                && x <= r.origin.x + r.size.width
                && r.origin.y <= y
                && y <= r.origin.y + r.size.height
        });
        if !inside {
            outside += 1;
        }
    }
    (changed, outside, damage)
}

#[test]
fn a_change_inside_a_translated_box_is_damaged_where_the_box_is_painted() {
    let (changed, outside, damage) = changed_outside_damage("translateX(-50%)");
    assert!(changed > 0, "harness: the recolour changes pixels");
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels lie outside the damage {damage:?}: the box is \
         painted translated, the damage must be too"
    );
}

#[test]
fn a_change_inside_a_scaled_box_is_damaged_where_the_box_is_painted() {
    let (changed, outside, damage) = changed_outside_damage("scale(2)");
    assert!(changed > 0, "harness: the recolour changes pixels");
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels lie outside the damage {damage:?}: the box is \
         painted scaled, the damage must be too"
    );
}
