//! A box sliding inside a transformed column is repainted where it is
//! painted, every frame of its slide.
//!
//! A slide (a FLIP move, an entering card) is a reference frame whose
//! matrix the frame driver moves by value, without a new display list; the
//! CPU frame damages the frame's content under its old and new matrix
//! (`gpu_value_damage`). It mapped the content through the sliding frame's
//! own matrix only, not through the frames around it: a card sliding in
//! ToastStack's column (`translateX(-50%)`) was repainted half a column
//! beside itself and left its old half on screen.

use azul_core::{
    resources::TransformKey,
    transform::ComputedTransform3D,
};
use azul_layout::{
    cpurender::{self, CpuRenderState, ScrollOffsetMap},
    solver3::display_list::{DisplayList, DisplayListItem, WindowLogicalRect},
};

use super::a_scroll_box_keeps_its_blit_on_a_scrolled_page::{
    fill, first_difference, rect, rgb, Raster,
};

fn translate(x: f32, y: f32) -> ComputedTransform3D {
    let mut m = ComputedTransform3D::IDENTITY;
    m.m[3][0] = x;
    m.m[3][1] = y;
    m
}

/// A grey page, a column at x 100 pulled back by 60 px (its own transform)
/// and a card in it on a sliding frame `slide`.
fn page(column: TransformKey, slide: TransformKey) -> DisplayList {
    let column_box = rect(100.0, 60.0, 120.0, 80.0);
    DisplayList {
        items: vec![
            fill(rect(0.0, 0.0, 200.0, 200.0), rgb(128, 128, 128)),
            DisplayListItem::PushReferenceFrame {
                transform_key: column,
                initial_transform: translate(-60.0, 0.0),
                bounds: WindowLogicalRect(column_box),
            },
            DisplayListItem::PushReferenceFrame {
                transform_key: slide,
                initial_transform: ComputedTransform3D::IDENTITY,
                bounds: WindowLogicalRect(rect(110.0, 70.0, 100.0, 40.0)),
            },
            fill(rect(110.0, 70.0, 100.0, 40.0), rgb(220, 30, 30)),
            DisplayListItem::PopReferenceFrame,
            DisplayListItem::PopReferenceFrame,
        ],
        ..Default::default()
    }
}

fn state(column: TransformKey, slide: TransformKey, y: f32) -> CpuRenderState {
    let mut state = CpuRenderState::new(ScrollOffsetMap::new());
    state.transforms.insert(column.id, translate(-60.0, 0.0));
    state.transforms.insert(slide.id, translate(0.0, y));
    state
}

#[test]
fn a_box_sliding_inside_a_transformed_column_is_repainted_where_it_is() {
    let (column, slide) = (TransformKey::unique(), TransformKey::unique());
    let dl = page(column, slide);
    let mut raster = Raster::new();
    let mut previous = state(column, slide, 0.0);
    let mut frame = raster.full_state(&dl, &previous);
    for y in [6.0f32, 14.0, 30.0, 22.0, 0.0] {
        let now = state(column, slide, y);
        let damage = cpurender::gpu_value_damage(
            &dl,
            &previous.transforms,
            &previous.opacities,
            &now.transforms,
            &now.opacities,
            &ScrollOffsetMap::new(),
        );
        assert!(!damage.needs_full, "a slide needs no full repaint");
        raster.repaint_state(&dl, &mut frame, &now, &damage.rects);
        let expected = raster.full_state(&dl, &now);
        assert_eq!(
            first_difference(&frame, &expected),
            None,
            "the card slid to y {y}: the frame must equal a full render (damage {:?})",
            damage.rects
        );
        previous = now;
    }
}
