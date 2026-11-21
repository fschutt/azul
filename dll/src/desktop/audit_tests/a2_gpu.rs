//! A2-gpu, signs and matrices on the GPU path, CPU values against GPU values.
//!
//! What `compositor2::translate_displaylist_to_wr` hands `WebRender` for a sample,
//! followed by hand through the spatial tree it built (reference frames: `M(p) +
//! origin`; scroll frames: content at minus their offset, which
//! `wr_translate2::scroll_all_nodes` sets to the scroll position x dpi), out to the
//! device pixel. The CPU rasteriser's pixels for the same samples are in azul-layout's
//! `audit_tests::a2_gpu` (same lists, same hand-computed numbers): the two must agree.
//! No GL context is needed for any of this: the translation builds a plain
//! `BuiltDisplayList`.

use std::collections::BTreeMap;

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::{DpiScaleFactor, RendererResources, TransformKey},
    styled_dom::StyledDom,
    transform::ComputedTransform3D,
};
use azul_css::props::basic::color::ColorU;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect},
    window::{DomLayoutResult, LayoutWindow},
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use webrender::api::{
    units::{DeviceIntSize, LayoutPoint, LayoutRect, LayoutTransform},
    BuiltDisplayList, DisplayItem, PipelineId, PropertyBinding, ReferenceTransformBinding,
    SpatialId, SpatialTreeItem,
};

use crate::desktop::compositor2::translate_displaylist_to_wr;

fn wlr(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h)).into()
}

fn within(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

/// `list` as `WebRender` receives it for the pipeline `pipeline`, at `dpi`.
fn translated(
    list: &DisplayList,
    pipeline: PipelineId,
    dpi: f32,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> BuiltDisplayList {
    let (_, built) = translate_displaylist_to_wr(
        list,
        pipeline,
        DeviceIntSize::new(1200, 640),
        &RendererResources::default(),
        DpiScaleFactor::new(dpi),
        Vec::new(),
        layout_results,
        0,
    )
    .expect("a balanced list translates");
    built
}

/// A node of the spatial tree `compositor2` built.
#[derive(Clone, Copy)]
enum Node {
    /// A reference frame: content is drawn at `transform(p) + origin` in `parent`.
    Frame {
        parent: SpatialId,
        origin: LayoutPoint,
        transform: LayoutTransform,
    },
    /// A scroll frame: content is drawn at `p - offset` in `parent`.
    Scroll { parent: SpatialId, scroll_id: u64 },
}

fn spatial_nodes(list: &BuiltDisplayList) -> Vec<(SpatialId, Node)> {
    let mut nodes = Vec::new();
    list.iter_spatial_tree(|item| match item {
        SpatialTreeItem::ReferenceFrame(f) => {
            let ReferenceTransformBinding::Static { binding } = &f.reference_frame.transform
            else {
                panic!("compositor2 opens static frames");
            };
            let transform = match binding {
                PropertyBinding::Value(t) | PropertyBinding::Binding(_, t) => *t,
            };
            nodes.push((
                f.reference_frame.id,
                Node::Frame {
                    parent: f.parent_spatial_id,
                    origin: f.origin,
                    transform,
                },
            ));
        }
        SpatialTreeItem::ScrollFrame(s) => nodes.push((
            s.scroll_frame_id,
            Node::Scroll {
                parent: s.parent_space,
                scroll_id: s.external_id.0,
            },
        )),
        _ => {}
    });
    nodes
}

/// The first rect of `list`: where it is in its own spatial node, and that node.
fn first_rect(list: &BuiltDisplayList) -> (LayoutRect, SpatialId) {
    let mut iter = list.iter();
    while let Some(item) = iter.next() {
        if let DisplayItem::Rectangle(r) = item.item() {
            return (r.bounds, r.common.spatial_id);
        }
    }
    panic!("the rect reaches WebRender");
}

/// Where `WebRender` draws `point` of the spatial node `node`, in device px of the
/// pipeline: through every frame around it, with the scroll frames scrolled by
/// `scroll` (scroll id -> the offset in DEVICE px that `scroll_all_nodes` sends).
fn drawn_at(
    nodes: &[(SpatialId, Node)],
    mut node: SpatialId,
    mut point: LayoutPoint,
    scroll: &BTreeMap<u64, (f32, f32)>,
) -> LayoutPoint {
    while let Some((_, found)) = nodes.iter().find(|(id, _)| *id == node) {
        match *found {
            Node::Frame {
                parent,
                origin,
                transform,
            } => {
                let moved = transform
                    .transform_point2d(point)
                    .expect("in front of the eye");
                point = LayoutPoint::new(moved.x + origin.x, moved.y + origin.y);
                node = parent;
            }
            Node::Scroll { parent, scroll_id } => {
                let (sx, sy) = scroll.get(&scroll_id).copied().unwrap_or((0.0, 0.0));
                point = LayoutPoint::new(point.x - sx, point.y - sy);
                node = parent;
            }
        }
    }
    point
}

fn framed_box(key: usize, matrix: ComputedTransform3D, b: WindowLogicalRect) -> Vec<DisplayListItem> {
    vec![
        DisplayListItem::PushReferenceFrame {
            transform_key: TransformKey { id: key },
            initial_transform: matrix,
            bounds: b,
        },
        DisplayListItem::Rect {
            bounds: b,
            color: ColorU::new(255, 0, 0, 255),
            border_radius: BorderRadius::default(),
        },
        DisplayListItem::PopReferenceFrame,
    ]
}

/// THE example of the area, on the GPU side: a 100 x 40 box at (300, 200),
/// `transform-origin: 50% 50%`, `scale(2)`, inside a scroll frame scrolled by (0, 50),
/// at 2 device px per logical px. By hand: the matrix about the box's corner is
/// `p -> 2 p - (50, 20)`; the corner goes to (250, 180), the scroll takes it to
/// (250, 130), the device pixel is (500, 260), and the far corner (900, 420).
/// The CPU rasteriser paints the same box there
/// (`a_scaled_box_in_a_scrolled_frame_is_painted_where_the_gpu_values_put_it`).
///
/// Mutations this fails: `LayoutPoint::new(-corner.x, -corner.y)` -> `+` for the inner
/// frame (x 1700), the outer frame opened at zero (x -100), `m[3][0] * dpi_scale` ->
/// `m[3][0]` (x 550), the scroll frame given a new offset origin for its items.
#[test]
fn a_scaled_box_in_a_scrolled_frame_reaches_webrender_where_the_cpu_paints_it() {
    let b = wlr(300.0, 200.0, 100.0, 40.0);
    let scale_about_centre = ComputedTransform3D::new_translation(-50.0, -20.0, 0.0)
        .then(&ComputedTransform3D::new_scale(2.0, 2.0, 1.0))
        .then(&ComputedTransform3D::new_translation(50.0, 20.0, 0.0));
    let mut items = vec![DisplayListItem::PushScrollFrame {
        clip_bounds: wlr(0.0, 0.0, 500.0, 300.0),
        content_size: LogicalSize::new(500.0, 2000.0),
        scroll_id: 7,
    }];
    items.extend(framed_box(9001, scale_about_centre, b));
    items.push(DisplayListItem::PopScrollFrame);
    let list = DisplayList {
        items,
        ..DisplayList::default()
    };

    for dpi in [1.0_f32, 2.0] {
        let built = translated(&list, PipelineId(0, 0), dpi, &BTreeMap::new());
        let nodes = spatial_nodes(&built);
        let (rect, node) = first_rect(&built);
        // What `scroll_all_nodes` sends for a scroll position of (0, 50): x dpi.
        let mut scroll = BTreeMap::new();
        scroll.insert(7_u64, (0.0, 50.0 * dpi));

        let top_left = drawn_at(&nodes, node, rect.min, &scroll);
        let bottom_right = drawn_at(&nodes, node, rect.max, &scroll);
        assert!(
            within(top_left.x, 250.0 * dpi, 0.01) && within(top_left.y, 130.0 * dpi, 0.01),
            "dpi {dpi}: the corner is drawn at {top_left:?}"
        );
        assert!(
            within(bottom_right.x, 450.0 * dpi, 0.01) && within(bottom_right.y, 210.0 * dpi, 0.01),
            "dpi {dpi}: the far corner is drawn at {bottom_right:?}"
        );
    }
}

/// A view's picture: fifteen stripes of 40 px, red first, 600 px in all.
extern "C" fn stripes(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; width: 600px; height: 300px;");
    for i in 0..15 {
        row = row.with_child(Dom::create_div().with_css(if i % 2 == 0 {
            "width: 40px; height: 300px; background: #ff0000;"
        } else {
            "width: 40px; height: 300px; background: #0000ff;"
        }));
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 300.0));
    VirtualViewReturn::with_dom(row, window, window)
}

/// A 600 x 320 window at 2x whose only content is a 300 x 300 view at (200, 0): the
/// window, the view's node and its child DOM.
fn window_with_a_view() -> (LayoutWindow, NodeId, DomId) {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(stripes))
            .with_css("position: absolute; left: 200px; top: 0px; width: 300px; height: 300px;"),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 320.0);
    ws.size.dpi = 192;
    lw.current_window_state = ws.clone();
    lw.layout_new_generation(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let (host, view) = *lw
        .virtual_view_manager
        .all_view_keys()
        .first()
        .expect("the page holds a view");
    assert_eq!(host, DomId::ROOT_ID);
    let child = lw
        .virtual_view_manager
        .get_nested_dom_id(host, view)
        .expect("the view's child DOM");
    (lw, view, child)
}

/// The iframe `compositor2` pushed for the view: its bounds, its clip, its pipeline.
fn iframe_of(list: &BuiltDisplayList) -> (LayoutRect, LayoutRect, PipelineId) {
    let mut iter = list.iter();
    while let Some(item) = iter.next() {
        if let DisplayItem::Iframe(i) = item.item() {
            return (i.bounds, i.clip_rect, i.pipeline_id);
        }
    }
    panic!("the view reaches WebRender as an iframe");
}

/// The Z1 class on the GPU side: where a panned view's content is drawn. The host's
/// translation only NAMES the child's pipeline, at the view's box x dpi - the content
/// offset is not in it - and the child's list is a pipeline of its own in its own
/// 0-relative coordinates x dpi; `scroll_all_nodes` scrolls that pipeline's root frame
/// by `virtual_view_frame_scroll_offsets() x dpi`, and `WebRender` draws scrolled content
/// at minus its offset.
///
/// By hand, at 2x: the view is at x 200 (device 400), the picture panned 37 px to the
/// left (scroll offset +37, device 74), the first blue stripe at child x 40 (device 80):
/// 400 + 80 - 74 = 406. The CPU rasteriser paints it from device x 406
/// (`a_panned_views_content_is_painted_where_the_gpu_scroll_offset_puts_it`).
///
/// Mutations this fails: the iframe pushed at `bounds + content_offset` (the offset
/// applied twice: 332), `-content.x` -> `content.x` in
/// `virtual_view_frame_scroll_offsets` (554), the child's items translated in the
/// host's coordinates (806).
#[test]
fn a_panned_views_content_reaches_webrender_where_the_cpu_paints_it() {
    let dpi = 2.0_f32;
    let (mut lw, view, child) = window_with_a_view();
    let host_pipeline = PipelineId(0, 0);
    let child_pipeline = PipelineId(child.inner as u32, 0);

    let before = iframe_of(&translated(
        &lw.layout_results[&DomId::ROOT_ID].display_list,
        host_pipeline,
        dpi,
        &lw.layout_results,
    ));
    lw.apply_virtual_view_geometry(
        DomId::ROOT_ID,
        view,
        Some(LogicalRect::new(
            LogicalPosition::new(-37.0, 0.0),
            LogicalSize::new(600.0, 300.0),
        )),
        None,
    );
    let (bounds, clip, pipeline) = iframe_of(&translated(
        &lw.layout_results[&DomId::ROOT_ID].display_list,
        host_pipeline,
        dpi,
        &lw.layout_results,
    ));
    assert_eq!(pipeline, child_pipeline, "the iframe names the child's pipeline");
    assert_eq!((bounds, clip, pipeline), before, "a pan is not in the host's translation");
    assert!(
        within(bounds.min.x, 400.0, 0.01)
            && within(bounds.min.y, 0.0, 0.01)
            && within(bounds.width(), 600.0, 0.01)
            && within(bounds.height(), 600.0, 0.01),
        "the iframe is the view's box x dpi: {bounds:?}"
    );
    assert_eq!(clip, bounds, "and it is clipped to that box");

    // The child's list, a pipeline of its own: its first blue stripe.
    let built_child = translated(
        &lw.layout_results[&child].display_list,
        child_pipeline,
        dpi,
        &lw.layout_results,
    );
    let mut blue = None;
    let mut iter = built_child.iter();
    while let Some(item) = iter.next() {
        if let DisplayItem::Rectangle(r) = item.item() {
            let color = match r.color {
                PropertyBinding::Value(c) | PropertyBinding::Binding(_, c) => c,
            };
            if color.b > 0.9 && color.r < 0.1 && color.g < 0.1 {
                blue = Some((r.bounds, r.common.spatial_id));
                break;
            }
        }
    }
    let (stripe, node) = blue.expect("the child's list paints a blue stripe");
    // The stripe is moved by the pan and by nothing else: it is in the root scroll frame
    // of the child's pipeline - the one a pan scrolls - or under it through scroll frames
    // of the list that move nothing themselves (the view's own frame of audit A4-08, which
    // has no scroll range and is never given an offset; `AZ_WR_VIEW_SLICE=0` leaves it
    // out). A reference frame on the way, or a frame with a range, would put the stripe
    // somewhere else than the CPU paints it.
    let pan_frame = SpatialId::root_scroll_node(child_pipeline);
    let mut space = node;
    let mut frames_between = 0;
    while space != pan_frame {
        let mut parent = None;
        built_child.iter_spatial_tree(|item| {
            if let webrender::api::SpatialTreeItem::ScrollFrame(frame) = item {
                if frame.scroll_frame_id == space {
                    assert_eq!(
                        frame.content_rect, frame.frame_rect,
                        "a scroll frame between the stripe and the pan's frame has no range"
                    );
                    parent = Some(frame.parent_space);
                }
            }
        });
        space = parent.expect(
            "the stripe is under the root scroll frame of the child's pipeline, the one a pan \
             scrolls, through scroll frames only",
        );
        frames_between += 1;
        assert!(frames_between <= 2, "at most the view's own frame is between them");
    }
    assert!(
        within(stripe.min.x, 80.0, 0.01),
        "in the child's own coordinates x dpi: {stripe:?}"
    );

    // The scroll `scroll_all_nodes` sends for that frame.
    let frame_scroll = lw
        .virtual_view_frame_scroll_offsets()
        .into_iter()
        .find(|(dom, _)| *dom == child)
        .map(|(_, offset)| offset)
        .expect("the child's pipeline is scrolled");
    let drawn_x = bounds.min.x + stripe.min.x - frame_scroll.x * dpi;
    assert!(
        within(drawn_x, 406.0, 0.01),
        "the first blue stripe is drawn from device x {drawn_x} (iframe {}, stripe {}, scroll \
         {frame_scroll:?})",
        bounds.min.x,
        stripe.min.x
    );
}

/// A2-gpu-01. NEEDS the change of `audit/A2-gpu.md`, "For area A4", item A2-gpu-01
/// (`compositor2.rs`, `PushReferenceFrame`: `initial_transform.m[0][3]`, `m[1][3]` and
/// `m[2][3]` divided by `dpi_scale`; the same three lines in
/// `wr_translate2::synchronize_gpu_values`): until it is applied this test fails at 2x
/// and says so.
///
/// A 100 x 40 box at (300, 200) under `perspective(400px) rotateY(60deg)` about its
/// centre. By hand: its top-right corner, logical (100, 0) from the box's corner, is
/// drawn at (72.558, 1.954) from it; at 2 device px per logical px that is device
/// (745.116, 403.907). With only the translation of the matrix scaled by the dpi the
/// frame's `w` is `1 + 2 x 100 x m03` instead of `1 + 100 x m03` and the corner lands
/// at (721.400, 403.269), 23.7 device px short - while the CPU rasteriser, which divides
/// the perspective terms by the dpi, paints it at 745
/// (`a_box_under_perspective_is_painted_where_the_gpu_values_put_it_at_two_device_px`).
///
/// Mutation: `initial_transform.m[0][3] / dpi_scale` -> `initial_transform.m[0][3]` in
/// `compositor2`'s `PushReferenceFrame` arm fails this test.
#[test]
fn a_box_under_perspective_reaches_webrender_where_the_cpu_paints_it_at_two_device_px() {
    use azul_core::transform::RotationMode;
    use azul_css::props::{
        basic::{angle::AngleValue, pixel::PixelValue},
        style::{StyleTransform, StyleTransformOrigin},
    };

    let b = wlr(300.0, 200.0, 100.0, 40.0);
    let tilt = ComputedTransform3D::from_style_transform_vec(
        &[
            StyleTransform::Perspective(PixelValue::px(400.0)),
            StyleTransform::RotateY(AngleValue::deg(60.0)),
        ],
        &StyleTransformOrigin::default(),
        100.0,
        40.0,
        RotationMode::ForWebRender,
    );
    let list = DisplayList {
        items: framed_box(9002, tilt, b),
        ..DisplayList::default()
    };

    for dpi in [1.0_f32, 2.0] {
        let built = translated(&list, PipelineId(0, 0), dpi, &BTreeMap::new());
        let nodes = spatial_nodes(&built);
        let (rect, node) = first_rect(&built);
        let top_right = drawn_at(
            &nodes,
            node,
            LayoutPoint::new(rect.max.x, rect.min.y),
            &BTreeMap::new(),
        );
        assert!(
            within(top_right.x, 372.558 * dpi, 0.05) && within(top_right.y, 201.954 * dpi, 0.05),
            "dpi {dpi}: the box's top-right corner is drawn at {top_right:?}, the CPU paints it \
             at ({}, {}). At 2x, (721.4, 403.3) is the matrix with only its translation scaled: \
             apply audit/A2-gpu.md, For area A4, A2-gpu-01 (divide m[0][3], m[1][3], m[2][3] by \
             dpi_scale in compositor2's PushReferenceFrame)",
            372.558 * dpi,
            201.954 * dpi
        );

        // The matrix the frame is opened with is the one `to_device_px` gives: the one
        // definition of the conversion, which `synchronize_gpu_values` has to send too.
        let want = tilt.to_device_px(dpi);
        let opened = nodes
            .iter()
            .find_map(|(_, n)| match n {
                Node::Frame { transform, .. } if !within(transform.m11, 1.0, 1e-6) => {
                    Some(*transform)
                }
                _ => None,
            })
            .expect("the transformed frame");
        let got = [
            [opened.m11, opened.m12, opened.m13, opened.m14],
            [opened.m21, opened.m22, opened.m23, opened.m24],
            [opened.m31, opened.m32, opened.m33, opened.m34],
            [opened.m41, opened.m42, opened.m43, opened.m44],
        ];
        for row in 0..4 {
            for column in 0..4 {
                assert!(
                    within(got[row][column], want.m[row][column], 1e-5),
                    "dpi {dpi}: m[{row}][{column}] of the frame is {}, in device px it is {}",
                    got[row][column],
                    want.m[row][column]
                );
            }
        }
    }
}
