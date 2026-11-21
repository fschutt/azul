//! A2-gpu, signs and matrices on the GPU path, CPU values against GPU values.
//!
//! The two renderers are separate consumers of the same values: a display list in
//! logical px, a matrix per reference frame (about the frame's own corner, translation
//! in logical px), a scroll offset per scroll frame, a content offset per `VirtualView`.
//! `WebRender` is handed them by `compositor2` / `wr_translate2` (azul-dll); the CPU
//! rasteriser reads them in `cpurender`. These tests paint a sample with the CPU
//! rasteriser, compute where the GPU translation's values put the same sample (the
//! formulas of `audit/A2-gpu.md`, "Spaces"), and require one device pixel for both.
//! The other half - that the translation really hands `WebRender` those values - is
//! `azul-dll`'s `desktop::audit_tests::a2_gpu`.

// The window and the CPU rasteriser are behind these two (both default) features.
#![cfg(all(feature = "text_layout", feature = "cpurender"))]

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::{RendererResources, TransformKey},
    styled_dom::StyledDom,
    transform::{ComputedTransform3D, RotationMode},
};
use azul_css::props::{
    basic::{AngleValue, ColorU, FontRef, PixelValue},
    style::{StyleTransform, StyleTransformOrigin},
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{
        render_with_font_manager_and_scroll, AzulPixmap, CpuRenderState, RenderOptions,
        ScrollOffsetMap,
    },
    glyph_cache::GlyphCache,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect},
    text3::cache::FontManager,
    window::LayoutWindow,
    window_state::FullWindowState,
};

const RED: ColorU = ColorU {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

fn wlr(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h)).into()
}

/// `list` painted by the CPU rasteriser into a white `width` x `height` (logical px)
/// frame at `dpi`.
fn paint(
    list: &DisplayList,
    state: &CpuRenderState,
    width: f32,
    height: f32,
    dpi: f32,
) -> AzulPixmap {
    let font_manager: FontManager<FontRef> =
        FontManager::new(FcFontCache::default()).expect("a font manager without faces");
    let mut glyph_cache = GlyphCache::new();
    render_with_font_manager_and_scroll(
        list,
        &RendererResources::default(),
        &font_manager,
        RenderOptions {
            width,
            height,
            dpi_factor: dpi,
        },
        &mut glyph_cache,
        state,
    )
    .expect("the list paints")
}

fn px(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    let d = p.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]]
}

/// More than half covered by the red box (over white): what a resampled edge pixel is
/// counted by.
fn mostly_red(c: [u8; 4]) -> bool {
    c[0] > 200 && c[1] < 128 && c[2] < 128
}

/// The device-pixel box `(x0, y0, x1, y1)`, half-open, of the pixels `is` holds for.
fn extent_of(p: &AzulPixmap, is: impl Fn([u8; 4]) -> bool) -> (u32, u32, u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..p.height {
        for x in 0..p.width {
            if is(px(p, x, y)) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    assert!(x1 > x0 && y1 > y0, "the sample is painted somewhere");
    (x0, y0, x1, y1)
}

fn within(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

/// Where the GPU translation's values put the point `p` (logical px, list space) of an
/// item inside ONE reference frame (`matrix` about `corner`, the frame's `bounds.origin`)
/// inside scroll frames scrolled by `scroll` in all, at `dpi`: the device pixel
/// `WebRender` draws it at.
///
/// `compositor2` hands every coordinate over as logical px x dpi, opens the frame at
/// `corner x dpi` with the matrix in device px (`ComputedTransform3D::to_device_px`)
/// and a plain frame at minus that corner inside it, and `scroll_all_nodes` scrolls
/// the frame around by `scroll x dpi` (`WebRender` draws scrolled content at minus
/// its offset): `corner_d + M_d (p_d - corner_d) - scroll_d`.
fn gpu_device_point(
    matrix: &ComputedTransform3D,
    corner: LogicalPosition,
    p: LogicalPosition,
    scroll: (f32, f32),
    dpi: f32,
) -> (f32, f32) {
    let in_frame = LogicalPosition::new((p.x - corner.x) * dpi, (p.y - corner.y) * dpi);
    let moved = matrix
        .to_device_px(dpi)
        .transform_point2d(in_frame)
        .expect("in front of the eye");
    (
        corner.x * dpi + moved.x - scroll.0 * dpi,
        corner.y * dpi + moved.y - scroll.1 * dpi,
    )
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
            color: RED,
            border_radius: BorderRadius::default(),
        },
        DisplayListItem::PopReferenceFrame,
    ]
}

/// THE example of the area: a 100 x 40 box at (300, 200), `transform-origin: 50% 50%`,
/// `scale(2)`, inside a scroll frame scrolled by (0, 50), at 2 device px per logical px.
///
/// By hand: the matrix about the box's corner is `p -> 2 p - (50, 20)`, so the corner
/// goes to (300, 200) + (-50, -20) = (250, 180), the scroll takes it to (250, 130), and
/// the device pixel is (500, 260); the box is 400 x 160 device px from there.
///
/// Mutations this fails: the scroll offset added instead of subtracted in the CPU walk
/// (y 460), the origin applied twice (x 400) or not at all (x 600), the translation of
/// the matrix not scaled by the dpi (x 550).
#[test]
fn a_scaled_box_in_a_scrolled_frame_is_painted_where_the_gpu_values_put_it() {
    let b = wlr(300.0, 200.0, 100.0, 40.0);
    let scale_about_centre = ComputedTransform3D::from_style_transform_vec(
        &[StyleTransform::Scale(
            azul_css::props::style::StyleTransformScale2D {
                x: azul_css::props::basic::FloatValue::new(2.0),
                y: azul_css::props::basic::FloatValue::new(2.0),
            },
        )],
        &StyleTransformOrigin::default(),
        100.0,
        40.0,
        RotationMode::ForWebRender,
    );
    assert!(
        within(scale_about_centre.m[3][0], -50.0, 1e-3)
            && within(scale_about_centre.m[3][1], -20.0, 1e-3)
            && within(scale_about_centre.m[0][0], 2.0, 1e-5),
        "p -> 2 p - (50, 20): {scale_about_centre:?}"
    );

    let mut items = vec![DisplayListItem::PushScrollFrame {
        clip_bounds: wlr(0.0, 0.0, 500.0, 300.0),
        content_size: LogicalSize::new(500.0, 2000.0),
        scroll_id: 7,
    }];
    items.extend(framed_box(9001, scale_about_centre, b));
    items.push(DisplayListItem::PopScrollFrame);
    let list = DisplayList {
        items,
        ..Default::default()
    };

    let mut offsets = ScrollOffsetMap::new();
    offsets.insert(7, (0.0, 50.0));
    let state = CpuRenderState::new(offsets);

    for dpi in [1.0_f32, 2.0] {
        let frame = paint(&list, &state, 500.0, 300.0, dpi);
        let (x0, y0, x1, y1) = extent_of(&frame, mostly_red);

        let corner = LogicalPosition::new(300.0, 200.0);
        let top_left = gpu_device_point(&scale_about_centre, corner, corner, (0.0, 50.0), dpi);
        let bottom_right = gpu_device_point(
            &scale_about_centre,
            corner,
            LogicalPosition::new(400.0, 240.0),
            (0.0, 50.0),
            dpi,
        );
        // The hand-computed place.
        assert!(
            within(top_left.0, 250.0 * dpi, 0.01) && within(top_left.1, 130.0 * dpi, 0.01),
            "dpi {dpi}: the GPU values put the corner at {top_left:?}"
        );
        assert!(
            within(bottom_right.0, 450.0 * dpi, 0.01) && within(bottom_right.1, 210.0 * dpi, 0.01),
            "dpi {dpi}: and the far corner at {bottom_right:?}"
        );
        // One pixel of slack: the CPU resamples the group and its edge pixel is a mix.
        assert!(
            within(x0 as f32, top_left.0, 1.0)
                && within(y0 as f32, top_left.1, 1.0)
                && within(x1 as f32, bottom_right.0, 1.0)
                && within(y1 as f32, bottom_right.1, 1.0),
            "dpi {dpi}: the CPU paints the box at ({x0}, {y0})..({x1}, {y1}), the GPU values \
             put it at {top_left:?}..{bottom_right:?}"
        );
    }
}

/// A2-gpu-01. A 100 x 40 box at (300, 200) under `perspective(400px) rotateY(60deg)`
/// about its centre, at 2 device px per logical px. By hand (the matrix is in
/// `azul_core::transform::audit_a2_gpu_tests`): the box's right edge, logical x 100
/// from its corner, is drawn at 72.558 from it, device x = 2 x (300 + 72.558) = 745.1;
/// its left edge at 2 x (300 + 21.965) = 643.9. A matrix whose perspective column is
/// not divided by the dpi (only its translation scaled, as `compositor2` did) puts the
/// right edge at 600 + 121.4 = 721.4: 23.7 device px short.
///
/// The CPU rasteriser's pixels are the reference here (its `layer_h` divides the
/// perspective terms by the dpi); the GPU values must put the edges on them.
///
/// Mutation: `m[0][3] /= scale_factor` removed from `ComputedTransform3D::to_device_px`
/// (the GPU values are then the translation-only ones) fails this test.
#[test]
fn a_box_under_perspective_is_painted_where_the_gpu_values_put_it_at_two_device_px() {
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
        ..Default::default()
    };
    let state = CpuRenderState::new(ScrollOffsetMap::new());
    let corner = LogicalPosition::new(300.0, 200.0);

    for dpi in [1.0_f32, 2.0] {
        let frame = paint(&list, &state, 500.0, 300.0, dpi);
        let (x0, _, x1, _) = extent_of(&frame, mostly_red);

        let left = gpu_device_point(&tilt, corner, corner, (0.0, 0.0), dpi);
        let right = gpu_device_point(
            &tilt,
            corner,
            LogicalPosition::new(400.0, 200.0),
            (0.0, 0.0),
            dpi,
        );
        assert!(
            within(left.0, 321.965 * dpi, 0.05) && within(right.0, 372.558 * dpi, 0.05),
            "dpi {dpi}: the GPU values put the edges at x {} and {}",
            left.0,
            right.0
        );
        assert!(
            within(x0 as f32, left.0, 2.0) && within(x1 as f32, right.0, 2.0),
            "dpi {dpi}: the CPU paints the box from x {x0} to {x1}, the GPU values put its \
             edges at {} and {}",
            left.0,
            right.0
        );
    }

    // What the translation-only scaling would have given at 2x, and how far that is
    // from the painted edge: the difference the finding is about.
    let mut translation_only = tilt;
    translation_only.scale_for_dpi(2.0);
    let wrong = translation_only
        .transform_point2d(LogicalPosition::new(200.0, 0.0))
        .expect("in front of the eye");
    assert!(within(600.0 + wrong.x, 721.4, 0.1), "{wrong:?}");
    let frame = paint(&list, &state, 500.0, 300.0, 2.0);
    let (_, _, x1, _) = extent_of(&frame, mostly_red);
    assert!(
        (x1 as f32 - (600.0 + wrong.x)).abs() > 20.0,
        "the CPU paints the right edge at {x1}, not at the translation-only {}",
        600.0 + wrong.x
    );
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

/// A 600 x 320 window at `dpi` (96 = 1x) whose only content is a 300 x 300 view at
/// (200, 0): the window, the view's node and its child DOM.
fn window_with_a_view(dpi: u32) -> (LayoutWindow, NodeId, DomId) {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(stripes))
            .with_css("position: absolute; left: 200px; top: 0px; width: 300px; height: 300px;"),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 320.0);
    ws.size.dpi = dpi;
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

/// The view's item in the host's list: its box and the content offset the CPU reads.
fn view_item(lw: &LayoutWindow, child: DomId) -> (LogicalRect, LogicalPosition) {
    lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::VirtualView {
                child_dom_id,
                bounds,
                content_offset,
                ..
            } if *child_dom_id == child => Some((*bounds.inner(), *content_offset)),
            _ => None,
        })
        .expect("the host's list holds the view")
}

/// The Z1 class on the GPU side: a view's content offset. The CPU rasteriser paints the
/// child's list at `bounds.origin + content_offset` (the item's field); `WebRender`
/// draws the child's pipeline at the iframe's `bounds.origin` and
/// `wr_translate2::scroll_all_nodes` scrolls its root frame by
/// `virtual_view_frame_scroll_offsets() x dpi`, content drawn at MINUS that.
///
/// By hand: the view is at x 200, the picture panned 37 px to the left
/// (`content_offset.x = -37`), so the second stripe (child x 40, the first blue one)
/// starts at 200 - 37 + 40 = 203, device px 203 at 1x and 406 at 2x. Panned by 37.3 at
/// 2x the offset is snapped to the device grid (-37.5): 202.5, device px 405.
///
/// Mutations this fails: `-content.x` -> `content.x` in
/// `virtual_view_frame_scroll_offsets` (GPU 277 instead of 203), `- content_offset.x`
/// -> `+ content_offset.x` in the rasteriser's `VirtualView` arm (CPU 277), the snap in
/// `virtual_view_content_offset` dropped (the 2x fractional case: GPU 404.6, CPU 405).
#[test]
fn a_panned_views_content_is_painted_where_the_gpu_scroll_offset_puts_it() {
    for (dpi, shift, first_blue) in [(96_u32, -37.0_f32, 203_u32), (192, -37.0, 406), (192, -37.3, 405)]
    {
        let scale = dpi as f32 / 96.0;
        let (mut lw, view, child) = window_with_a_view(dpi);
        lw.apply_virtual_view_geometry(
            DomId::ROOT_ID,
            view,
            Some(LogicalRect::new(
                LogicalPosition::new(shift, 0.0),
                LogicalSize::new(600.0, 300.0),
            )),
            None,
        );

        // The stripe in the child's own list (child list space): harness check.
        let child_list = Arc::clone(&lw.layout_results[&child].display_list);
        let blue_x = child_list
            .items
            .iter()
            .find_map(|item| match item {
                DisplayListItem::Rect { bounds, color, .. } if color.b == 255 && color.r == 0 => {
                    Some(bounds.inner().origin.x)
                }
                _ => None,
            })
            .expect("harness: the child's list paints a blue stripe");
        assert!(
            within(blue_x, 40.0, 0.01),
            "harness: the first blue stripe is laid out at child x {blue_x}, not 40"
        );

        // The GPU's values: the iframe's place and the scroll of the child's pipeline.
        let (bounds, content_offset) = view_item(&lw, child);
        let frame_scroll = lw
            .virtual_view_frame_scroll_offsets()
            .into_iter()
            .find(|(dom, _)| *dom == child)
            .map(|(_, offset)| offset)
            .expect("the child's pipeline is scrolled");
        let gpu_x = bounds.origin.x * scale + blue_x * scale - frame_scroll.x * scale;
        assert!(
            within(gpu_x, first_blue as f32, 0.01),
            "dpi {dpi}, pan {shift}: the GPU values put the first blue stripe at {gpu_x}, by \
             hand {first_blue} (box {bounds:?}, scroll {frame_scroll:?})"
        );
        // The same number from the field the CPU reads.
        let cpu_x = (bounds.origin.x + content_offset.x + blue_x) * scale;
        assert!(
            within(cpu_x, gpu_x, 0.01),
            "dpi {dpi}, pan {shift}: content offset {content_offset:?} against scroll \
             {frame_scroll:?}"
        );

        // And the CPU's pixels.
        let mut lists = BTreeMap::new();
        lists.insert(child, child_list);
        let state =
            CpuRenderState::new(ScrollOffsetMap::new()).with_virtual_view_display_lists(lists);
        let host_list = Arc::clone(&lw.layout_results[&DomId::ROOT_ID].display_list);
        let frame = paint(&host_list, &state, 600.0, 320.0, scale);
        let row = (10.0 * scale) as u32;
        let is_blue = |c: [u8; 4]| c[0] < 60 && c[1] < 60 && c[2] > 200;
        let painted_blue = (0..frame.width)
            .find(|x| is_blue(px(&frame, *x, row)))
            .expect("a blue stripe is painted");
        assert_eq!(
            painted_blue, first_blue,
            "dpi {dpi}, pan {shift}: the CPU paints the first blue stripe from device x \
             {painted_blue}"
        );
        // Left of the view's box nothing of the picture shows.
        let outside = px(&frame, (199.0 * scale) as u32, row);
        assert!(
            !mostly_red(outside) && !is_blue(outside),
            "the view is clipped to its box: {outside:?} left of it"
        );
    }
}
