//! A sliding box carries its positioned children.
//!
//! A rebuild slides a box that declares `animation: move` by a transform on
//! the animation channel, painted through a reference frame around the box.
//! CSS Transforms 1 §2: a transformed box is a stacking context and the
//! containing block of its positioned descendants, so they move with it. A
//! box on the animation channel was no stacking context: its
//! `position: absolute` children were painted by the stacking context around
//! it, outside its reference frame, and jumped to their new place while the
//! box slid (AzPlayer's start strip: the category names jumped, only the row
//! boxes slid).
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::{RendererResources, TransformKey},
    styled_dom::StyledDom,
    transform::ComputedTransform3D,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SLIDER: NodeId = NodeId::new(1);

/// body > slider (`position: relative`, 100x50) > label (`position:
/// absolute`, 40x10, red).
fn page() -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body().with_css("margin: 0; padding: 0;").with_child(
            Dom::create_div()
                .with_css("position: relative; width: 100px; height: 50px;")
                .with_child(Dom::create_div().with_css(
                    "position: absolute; left: 0px; top: 0px; width: 40px; height: 10px; \
                     background: rgb(255, 0, 0);",
                )),
        ),
    )
}

/// The page laid out with the slider 30px down its slide (the animation
/// channel, as `tick_animations` publishes a FLIP), and its display list.
fn display_list_mid_slide() -> Vec<DisplayListItem> {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let lay_out = |lw: &mut LayoutWindow| {
        lw.layout_and_generate_display_list(
            page(),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
    };
    lay_out(&mut lw);
    let cache = lw.gpu_state_manager.get_or_create_cache(DomId::ROOT_ID);
    cache.anim_transform_keys.insert(SLIDER, TransformKey::unique());
    cache
        .anim_current_transform_values
        .insert(SLIDER, ComputedTransform3D::new_translation(0.0, 30.0, 0.0));
    // The key population changed: the next list binds the slider's frame.
    lay_out(&mut lw);
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .clone()
}

/// The index range `[push, pop]` of the first reference frame opened for a
/// box of `width` x `height`.
fn frame_of(items: &[DisplayListItem], width: f32, height: f32) -> Option<(usize, usize)> {
    let push = items.iter().position(|item| {
        matches!(item, DisplayListItem::PushReferenceFrame { bounds, .. }
            if (bounds.0.size.width - width).abs() < 0.5
                && (bounds.0.size.height - height).abs() < 0.5)
    })?;
    let mut depth = 0usize;
    for (i, item) in items.iter().enumerate().skip(push) {
        match item {
            DisplayListItem::PushReferenceFrame { .. } => depth += 1,
            DisplayListItem::PopReferenceFrame => {
                depth -= 1;
                if depth == 0 {
                    return Some((push, i));
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn a_sliding_box_paints_its_absolutely_positioned_child_inside_its_frame() {
    let items = display_list_mid_slide();
    let (push, pop) = frame_of(&items, 100.0, 50.0)
        .expect("the sliding box is painted through a reference frame of its own");
    let label = items
        .iter()
        .position(|item| {
            matches!(item, DisplayListItem::Rect { bounds, color, .. }
                if color.r == 255 && color.g == 0 && color.b == 0
                    && (bounds.0.size.width - 40.0).abs() < 0.5)
        })
        .expect("the label paints its red background");
    assert!(
        push < label && label < pop,
        "the label is painted inside the slider's frame (items {push}..{pop}) and slides with it, \
         but it is item {label}"
    );
}

#[test]
fn a_sliding_box_is_a_stacking_context_while_it_slides() {
    let items = display_list_mid_slide();
    let (push, _) = frame_of(&items, 100.0, 50.0).expect("the slider has its frame");
    assert!(
        matches!(
            items.get(push + 1),
            Some(DisplayListItem::PushStackingContext { .. })
        ),
        "a transformed box is a stacking context (CSS Transforms 1 §2): its frame opens one, got \
         {:?}",
        items.get(push + 1)
    );
}
