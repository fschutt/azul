//! Pin for the 2026-08-29 snap-back scrollbar trail: the scroll-shift memmove
//! drags every pixel inside the clip — including an overlay painted over the
//! frame (its own scrollbar) — and the damage recipe must repaint the overlay
//! at BOTH its fixed position and where its dragged ghost landed. The e2e
//! twin carried the ghost half; the shell did not, so a rubber-band snap-back
//! left one smeared scrollbar copy per frame. Both sides now share
//! `cpurender::execute_scroll_shift`, which this test pins.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_css::props::basic::ColorU;
use azul_layout::{
    cpurender::{self, AzulPixmap},
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect},
};

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition::new(x, y),
        size: LogicalSize::new(w, h),
    }
}

fn covers(damage: &[LogicalRect], target: &LogicalRect) -> bool {
    // The target must be fully inside SOME damaged rect (the recipe pushes
    // whole rects, not slivers).
    damage.iter().any(|d| {
        target.origin.x >= d.origin.x - 0.01
            && target.origin.y >= d.origin.y - 0.01
            && target.origin.x + target.size.width <= d.origin.x + d.size.width + 0.01
            && target.origin.y + target.size.height <= d.origin.y + d.size.height + 0.01
    })
}

#[test]
fn a_dragged_overlay_scrollbar_is_repainted_at_its_ghost_position_too() {
    let clip = rect(0.0, 0.0, 200.0, 100.0);
    // Opaque content big enough to cover the clip at the old AND new offset.
    let content = rect(-100.0, -100.0, 600.0, 500.0);
    // The overlay "scrollbar": a strip at the right edge, painted AFTER the
    // frame closes.
    let bar = rect(190.0, 0.0, 8.0, 100.0);

    let dl = DisplayList {
        items: vec![
            DisplayListItem::PushScrollFrame {
                clip_bounds: WindowLogicalRect(clip),
                content_size: LogicalSize::new(600.0, 500.0),
                scroll_id: 7,
            },
            DisplayListItem::Rect {
                bounds: WindowLogicalRect(content),
                color: ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::PopScrollFrame,
            DisplayListItem::Rect {
                bounds: WindowLogicalRect(bar),
                color: ColorU {
                    r: 128,
                    g: 128,
                    b: 128,
                    a: 255,
                },
                border_radius: BorderRadius::default(),
            },
        ],
        ..Default::default()
    };

    let mut pixmap = AzulPixmap::new(200, 100).expect("pixmap");
    let delta = (10.0, 0.0);
    let out = cpurender::execute_scroll_shift(
        &mut pixmap,
        &dl,
        7,
        &clip,
        delta,
        (10.0, 0.0),
        1.0,
        false,
    );

    // Fast path taken: the whole clip is presented, not repainted.
    assert!(
        out.present_extra.contains(&clip),
        "eligible frame must present the moved clip (fast path), damage: {:?}",
        out.damage
    );
    // The bar itself is repainted...
    assert!(
        covers(&out.damage, &bar),
        "the overlay must be repainted at its fixed position, damage: {:?}",
        out.damage
    );
    // ...and so is the position its pixels were DRAGGED to (origin - delta):
    // without this rect the snap-back leaves one smeared copy per frame.
    let ghost = rect(bar.origin.x - delta.0, bar.origin.y, bar.size.width, bar.size.height);
    assert!(
        covers(&out.damage, &ghost),
        "the dragged ghost at origin-delta must be repainted, damage: {:?}",
        out.damage
    );
}
