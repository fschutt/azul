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

// =========================================================================
// Nested frames: the fallback clip must be where the frame is ON SCREEN
// =========================================================================

/// The TextInput's value `<p>` is a scroll frame NESTED in the page column.
/// `PushScrollFrame.clip_bounds` is in the parent's content space; with the
/// page scrolled, the collector used to hand that unprojected clip on as the
/// nested frame's damage - `page offset` pixels away from the field, so the
/// field kept its old horizontal offset while a correctly placed caret strip
/// beside it rendered at the new one (the seam, 2026-08-31).
#[test]
fn a_nested_frames_shift_clip_is_projected_by_the_outer_scroll() {
    let page_clip = rect(0.0, 0.0, 400.0, 300.0);
    let field_clip = rect(20.0, 250.0, 300.0, 22.0); // page-content space
    let dl = DisplayList {
        items: vec![
            DisplayListItem::PushScrollFrame {
                clip_bounds: WindowLogicalRect(page_clip),
                content_size: LogicalSize::new(400.0, 2000.0),
                scroll_id: 1,
            },
            DisplayListItem::PushScrollFrame {
                clip_bounds: WindowLogicalRect(field_clip),
                content_size: LogicalSize::new(900.0, 22.0),
                scroll_id: 2,
            },
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopScrollFrame,
        ],
        ..Default::default()
    };
    // Page scrolled down by 100 (unchanged this frame); the field glides
    // horizontally from 40 to 48.
    let mut now = cpurender::ScrollOffsetMap::new();
    now.insert(1, (0.0, 100.0));
    now.insert(2, (48.0, 0.0));
    let mut prev = cpurender::ScrollOffsetMap::new();
    prev.insert(1, (0.0, 100.0));
    prev.insert(2, (40.0, 0.0));

    let shifts = cpurender::collect_scroll_shifts(&dl, &now, &prev, 2.0);
    assert_eq!(shifts.len(), 1, "only the field moved: {shifts:?}");
    let (id, clip, delta, offset) = shifts[0];
    assert_eq!(id, 2);
    assert_eq!(delta, (8.0, 0.0));
    assert_eq!(offset, (48.0, 0.0));
    assert!(
        (clip.origin.y - (field_clip.origin.y - 100.0)).abs() < 0.01
            && (clip.origin.x - field_clip.origin.x).abs() < 0.01,
        "the nested clip must be projected by the OUTER page offset: got {clip:?}"
    );

    // And a top-level frame's clip is untouched.
    let mut now2 = cpurender::ScrollOffsetMap::new();
    now2.insert(1, (0.0, 120.0));
    let shifts = cpurender::collect_scroll_shifts(&dl, &now2, &prev, 2.0);
    let page = shifts.iter().find(|s| s.0 == 1).expect("the page moved");
    assert_eq!(page.1, page_clip, "a top-level frame keeps its own clip");
}
