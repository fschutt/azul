//! A1, the CPU renderer (parallel, retained).
//!
//! The guards that need the rasteriser's private items are in `cpurender/raster.rs`
//! (`audit_a1_tests`), `cpurender/bands.rs` and `cpurender/retained.rs`; here are the ones that
//! do not.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_css::props::{
    basic::{
        pixel::{PixelValue, PixelValueNoPercent},
        ColorU,
    },
    style::box_shadow::{BoxShadowClipMode, StyleBoxShadow},
};

use crate::solver3::display_list::{BorderRadius, DisplayList, DisplayListItem};

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition { x, y },
        size: LogicalSize { width: w, height: h },
    }
}

fn push_frame(key: usize) -> DisplayListItem {
    DisplayListItem::PushReferenceFrame {
        transform_key: azul_core::resources::TransformKey { id: key },
        initial_transform: azul_core::transform::ComputedTransform3D {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        },
        bounds: rect(0.0, 0.0, 100.0, 50.0).into(),
    }
}

fn push_clip() -> DisplayListItem {
    DisplayListItem::PushClip {
        bounds: rect(0.0, 0.0, 100.0, 50.0).into(),
        border_radius: BorderRadius::default(),
    }
}

fn fill() -> DisplayListItem {
    DisplayListItem::Rect {
        bounds: rect(10.0, 10.0, 20.0, 20.0).into(),
        color: ColorU { r: 9, g: 9, b: 9, a: 255 },
        border_radius: BorderRadius::default(),
    }
}

fn text_shadow(offset: f32, blur: f32, spread: f32) -> StyleBoxShadow {
    let length = |v: f32| PixelValueNoPercent { inner: PixelValue::px(v) };
    StyleBoxShadow {
        offset_x: length(offset),
        offset_y: length(-offset),
        blur_radius: length(blur),
        spread_radius: length(spread),
        color: ColorU { r: 255, g: 255, b: 255, a: 255 },
        clip_mode: BoxShadowClipMode::Outset,
    }
}

/// A1-07 (h): what a walk may step over. A frame is self-contained when every scope opened in
/// it is closed in it, innermost first; a leaf when no frame, scroll frame or view is inside;
/// and it knows how far a text shadow inside it reaches (2 + 3 + 4 = 9 px here).
/// Mutations: `scopes.len() == done.scopes_before + 1` -> `>=` calls the frame with the clip
/// left open self-contained (a walk stepping over it would leave the clip's pop without its
/// push); marking `leaf` after the new frame is opened makes a frame its own non-leaf.
#[test]
fn a_frame_knows_whether_a_walk_may_step_over_it() {
    use DisplayListItem as I;
    let items = vec![
        // 1: a clip opened and closed inside
        push_frame(1),
        push_clip(),
        fill(),
        I::PopClip,
        I::PopReferenceFrame,
        // 2: a clip left open at the pop, closed after it
        push_frame(2),
        push_clip(),
        I::PopReferenceFrame,
        I::PopClip,
        // 3 around 4: 4 holds a text shadow
        push_frame(3),
        push_frame(4),
        I::PushTextShadow { shadow: text_shadow(2.0, 3.0, 4.0) },
        I::PopTextShadow,
        I::PopReferenceFrame,
        I::PopReferenceFrame,
        // 5: a pop of a clip that was opened before the frame
        push_clip(),
        push_frame(5),
        I::PopClip,
        I::PopReferenceFrame,
        // 6: never popped
        push_frame(6),
        fill(),
    ];
    let list = DisplayList { items, ..Default::default() };
    let index = list.frame_index();
    let frame = |key: usize| {
        let push = list
            .items
            .iter()
            .position(|item| matches!(item, I::PushReferenceFrame { transform_key, .. } if transform_key.id == key))
            .expect("the frame is in the list");
        index.frame_at(push).expect("and in the index")
    };
    let f1 = frame(1);
    assert_eq!((f1.push, f1.end), (0, 4));
    assert!(f1.self_contained && f1.leaf);
    assert_eq!(f1.shadow_reach, 0.0);
    let f2 = frame(2);
    assert_eq!((f2.push, f2.end), (5, 7));
    assert!(!f2.self_contained, "its clip is still open at its pop");
    let (f3, f4) = (frame(3), frame(4));
    assert_eq!((f3.end, f4.end), (14, 13));
    assert!(f3.self_contained && f4.self_contained);
    assert!(!f3.leaf, "a frame inside");
    assert!(f4.leaf);
    assert_eq!((f3.shadow_reach, f4.shadow_reach), (9.0, 9.0), "offset 2, blur 3, spread 4");
    let f5 = frame(5);
    assert!(!f5.self_contained, "it closes a clip opened before it");
    let f6 = frame(6);
    assert_eq!(f6.end, list.items.len());
    assert!(!f6.self_contained, "never popped");
}

/// A1-06 (a): the glyph caches of the band threads are taken out with the window's text
/// settings, kept for the next frame, and fewer threads keep fewer.
/// Mutation: `take_band_caches` without the settings paints a band's text with the default
/// anti-aliasing next to the window's.
#[cfg(feature = "cpurender")]
#[test]
fn the_band_threads_glyph_caches_are_kept_with_the_windows_settings() {
    use crate::{cpurender::TextAaSettings, glyph_cache::GlyphCache};
    let mut cache = GlyphCache::new();
    let grayscale = TextAaSettings::grayscale();
    let _ = cache.set_text_aa(grayscale);
    let bands = cache.take_band_caches(3);
    assert_eq!(bands.len(), 3);
    assert_eq!(cache.band_caches_len(), 0, "taken out while the bands run");
    assert!(bands.iter().all(|band| band.text_aa() == grayscale));
    cache.put_band_caches(bands);
    assert_eq!(cache.band_caches_len(), 3);
    let bands = cache.take_band_caches(1);
    assert_eq!(bands.len(), 1, "one more thread, one cache");
    cache.put_band_caches(bands);
    assert_eq!(cache.band_caches_len(), 1);
    cache.gc();
    assert_eq!(cache.band_caches_len(), 1, "an idle frame frees their old generation, not them");
}
