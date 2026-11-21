//! A10-06, the layer-backed present, through `CpuBackend::render_frame`: a window with a view
//! whose picture is promoted to a layer (`cpurender::present_layer`).
//!
//! The walk-level guards (the planner's geometry, the pixel invariant against the walk that
//! composites the group itself) are in `cpurender/present_layer.rs`. These are the frame's:
//! that a real window's lists promote the picture at all, that a zoom step and a pan of the
//! view paint no pixel of the window's frame and only move the layer, and that the window's
//! frame with the layer over it is what a backend without the switch paints.
//!
//! The window is the one of `e2e/cpu_view_values_tests.rs` (400 x 300, a view at (50, 40),
//! 300 x 200), with a picture that is larger than the view, as a map's is: 700 x 500 at
//! (-100, -100) of the view's content, carrying an identity transform at build time.

#![cfg(feature = "cpurender")]

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::{FloatValue, PixelValue},
    property::CssProperty,
    style::{StyleTransform, StyleTransformScale2D, StyleTransformTranslate2D},
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    cpu_backend::CpuBackend,
    cpurender::{
        blit_pixmap_affine_clipped, mat3_affine_part,
        present_layer::{LayerMatrix, PresentLayer},
        AzulPixmap, Mat3,
    },
    overlay::ContentChange,
    window::{FrameDamage, LayoutWindow},
    window_state::FullWindowState,
};

/// The view's child: the picture, 700 x 500 at (-100, -100), a ground with six cells on it.
extern "C" fn page(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let cell = |x: u32, y: u32, colour: &str| {
        Dom::create_div().with_css(&format!(
            "position: absolute; left: {x}px; top: {y}px; width: 60px; height: 40px; background: {colour};"
        ))
    };
    let picture = Dom::create_div()
        .with_class("picture".into())
        .with_css(
            "position: absolute; left: -100px; top: -100px; width: 700px; height: 500px; \
             background: #e8e4d8; transform: translate(0px, 0px) scale(1);",
        )
        .with_child(cell(110, 110, "#c03030"))
        .with_child(cell(250, 150, "#30a040"))
        .with_child(cell(330, 250, "#3050c0"))
        .with_child(cell(420, 120, "#c0a020"))
        .with_child(cell(20, 20, "#602080"))
        .with_child(cell(620, 440, "#108090"));
    let dom = Dom::create_div().with_child(picture);
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.get_logical_size());
    VirtualViewReturn::with_dom(dom, rect, rect)
}

struct Window {
    lw: LayoutWindow,
    resources: RendererResources,
    host: DomId,
    view: NodeId,
    child: DomId,
    picture: NodeId,
}

fn window() -> Window {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(page))
            .with_css("position: absolute; left: 50px; top: 40px; width: 300px; height: 200px;"),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let resources = RendererResources::default();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &resources,
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let (host, view) = *lw
        .virtual_view_manager
        .all_view_keys()
        .first()
        .expect("the page holds a view");
    let child = lw
        .virtual_view_manager
        .get_nested_dom_id(host, view)
        .expect("the view's DOM");
    let nodes = lw.layout_results[&child].styled_dom.node_data.as_container();
    let picture = (0..nodes.len())
        .map(NodeId::new)
        .find(|n| nodes[*n].has_class("picture"))
        .expect("the picture's node");
    Window { lw, resources, host, view, child, picture }
}

/// `translate(tx, ty) scale(s)`, the editor's zoom.
fn zoom(tx: f32, ty: f32, s: f32) -> CssProperty {
    CssProperty::transform(
        vec![
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::px(tx),
                y: PixelValue::px(ty),
            }),
            StyleTransform::Scale(StyleTransformScale2D {
                x: FloatValue::new(s),
                y: FloatValue::new(s),
            }),
        ]
        .into(),
    )
}

impl Window {
    /// The write `set_css_property` makes to the picture.
    fn write(&mut self, prop: CssProperty) {
        let _ = self.lw.apply_content_change(ContentChange::NodeCss {
            dom_id: self.child,
            node_id: self.picture,
            props: vec![prop],
            override_only: false,
        });
    }

    /// The editor's pan at scale 1 (`update_virtual_view`): the view's content at `(x, y)`.
    fn pan(&mut self, x: f32, y: f32) {
        self.lw.apply_virtual_view_geometry(
            self.host,
            self.view,
            Some(LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(300.0, 200.0))),
            None,
        );
    }

    fn frame(&self, backend: &mut CpuBackend) -> AzulPixmap {
        backend.render_frame(&self.lw, &self.resources, 400.0, 300.0, 1.0);
        backend
            .last_frame
            .as_ref()
            .expect("the backend painted a frame")
            .clone_pixmap()
    }

    /// The kinds of the items of every list of the window, for a message.
    fn lists(&self) -> String {
        let mut out = String::new();
        for (dom, result) in &self.lw.layout_results {
            let kinds: Vec<String> = result
                .display_list
                .items
                .iter()
                .map(|item| {
                    let debug = format!("{item:?}");
                    debug
                        .split(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or("")
                        .to_string()
                })
                .collect();
            out.push_str(&format!("\n  dom {}: {}", dom.inner, kinds.join(" ")));
        }
        out
    }
}

/// What the shell shows: the window's frame with the layer's surface through its matrix
/// inside its clip (the composite of a group, `blit_pixmap_affine_clipped`).
fn shown(frame: &AzulPixmap, backend: &CpuBackend) -> AzulPixmap {
    let mut out = frame.clone_pixmap();
    if let Some(layer) = backend.last_present_layer.as_ref() {
        let m = layer.matrix;
        let mat3: Mat3 = [m.xx, m.xy, m.tx, m.yx, m.yy, m.ty, 0.0, 0.0, 1.0];
        let composited = backend.with_present_layer_surface(|surface| {
            assert_eq!((surface.width(), surface.height()), layer.surface_size);
            blit_pixmap_affine_clipped(surface, &mut out, &mat3_affine_part(&mat3), 1.0, Some(layer.clip));
        });
        assert!(composited.is_some(), "a reported layer has a surface");
    }
    out
}

/// The pixels where two frames differ by more than a rounding (one level a channel).
fn differing(a: &AzulPixmap, b: &AzulPixmap) -> usize {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    a.data()
        .chunks_exact(4)
        .zip(b.data().chunks_exact(4))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 1))
        .count()
}

/// The area a frame painted of the window's frame, logical px squared.
fn painted_area(backend: &CpuBackend) -> f32 {
    match &backend.last_frame_damage {
        FrameDamage::None => 0.0,
        FrameDamage::Rects(rects) => rects.iter().map(|r| r.size.width * r.size.height).sum(),
        FrameDamage::Full => f32::INFINITY,
    }
}

fn layer_of(backend: &CpuBackend, w: &Window, what: &str) -> PresentLayer {
    backend.last_present_layer.clone().unwrap_or_else(|| {
        panic!(
            "{what}: the picture is promoted to a layer. The window's lists (a frame is promoted \
             when it is the largest whose surface can be kept, in no group, mask, rounded clip \
             or text shadow, with nothing painted over it):{}",
            w.lists()
        )
    })
}

/// A10-06 (R3). The frames of a window whose picture is a layer:
///
/// 1. The first frame promotes the picture. The view is at (50, 40) and the picture at
///    (-100, -100) of its content: the surface's pixel (0, 0) lands at (50 - 100, 40 - 100) =
///    (-50, -60), under the view's box (50, 40) .. (350, 240). The surface is 700 x 500.
/// 2. A zoom step (the transform written to the picture: no list, no layout) paints NO pixel
///    of the window's frame and none of the surface: the layer's matrix alone is another.
/// 3. So does a pan during the zoom.
/// 4. Back at the identity, a pan of the view (its content offset, as the editor pans at
///    scale 1) moves the layer by the pan and paints nothing: (12, 5) -> (-38, -55).
/// At every step the window's frame with the layer over it is the frame a backend without
/// the switch paints (to a rounding).
///
/// Mutations: without `promoted_frame_moved` step 2 is skipped as idle and the layer keeps
/// its matrix (the frame-and-layer assert of step 2); with the view's shift executed in step
/// 4, or the promoted frame's value damage left in, the painted area is the view's box.
#[test]
fn a_zoom_step_and_a_pan_of_a_promoted_picture_paint_nothing() {
    let mut w = window();
    let mut layered = CpuBackend::new();
    layered.promote_for_present = true;
    let mut plain = CpuBackend::new();

    let check = |w: &Window, layered: &mut CpuBackend, plain: &mut CpuBackend, what: &str| {
        let below = w.frame(layered);
        let wanted = w.frame(plain);
        let layer = layer_of(layered, w, what);
        assert_eq!(
            differing(&shown(&below, layered), &wanted),
            0,
            "{what}: the window's frame with its layer is the frame without the switch"
        );
        layer
    };

    let first = check(&w, &mut layered, &mut plain, "the first frame");
    assert_eq!(first.surface_size, (700, 500));
    assert_eq!(first.clip, (50, 40, 350, 240));
    assert_eq!(first.matrix, LayerMatrix { xx: 1.0, xy: 0.0, yx: 0.0, yy: 1.0, tx: -50.0, ty: -60.0 });
    assert!(plain.last_present_layer.is_none(), "without the switch no layer is reported");

    w.write(zoom(-20.0, -10.0, 1.25));
    let zoomed = check(&w, &mut layered, &mut plain, "a zoom step");
    assert_eq!(painted_area(&layered), 0.0, "a zoom step paints no pixel: {:?}", layered.last_frame_damage);
    assert_ne!(layered.last_frame_damage, FrameDamage::Full);
    assert_eq!(zoomed.generation, first.generation, "nor a pixel of the surface");
    assert_eq!(zoomed.surface_id, first.surface_id, "the same picture");
    assert_eq!((zoomed.matrix.xx, zoomed.matrix.yy), (1.25, 1.25), "the layer is scaled");
    assert_eq!(zoomed.clip, first.clip);

    w.write(zoom(-60.0, -30.0, 1.25));
    let panned = check(&w, &mut layered, &mut plain, "a pan during the zoom");
    assert_eq!(painted_area(&layered), 0.0, "a pan during a zoom paints no pixel: {:?}", layered.last_frame_damage);
    assert_eq!(panned.generation, first.generation);
    assert_eq!(
        (
            (panned.matrix.tx - zoomed.matrix.tx).round(),
            (panned.matrix.ty - zoomed.matrix.ty).round()
        ),
        (-40.0, -20.0),
        "the layer moves by what the translation moved"
    );

    w.write(zoom(0.0, 0.0, 1.0));
    let back = check(&w, &mut layered, &mut plain, "back at the identity");
    assert_eq!(back.matrix, first.matrix);
    assert_eq!(back.generation, first.generation);

    w.pan(12.0, 5.0);
    let moved = check(&w, &mut layered, &mut plain, "a pan of the view");
    assert_eq!(
        (moved.matrix.tx, moved.matrix.ty),
        (-38.0, -55.0),
        "the layer moves with the view's content"
    );
    assert_eq!(
        painted_area(&layered),
        0.0,
        "a pan of a view that shows the picture alone paints no pixel: {:?} (a rect of the view's \
         box means the view's list paints something besides the picture){}",
        layered.last_frame_damage,
        w.lists()
    );
    assert_eq!(moved.generation, first.generation);

    // Nothing changes any more: the frame is idle, and the layer stays reported.
    let _ = w.frame(&mut layered);
    assert_eq!(layered.last_frame_damage, FrameDamage::None);
    assert_eq!(layered.last_present_layer.as_ref(), Some(&moved));
}

/// A10-06, the switch-over, both ways, and the release of the kept surfaces. When the
/// promotion stops, the window's frame gains the picture under the layer's clip (ONE repaint
/// of the clip) and no layer is reported; when it starts again, it loses it. After
/// `release_caches` the next frame is painted in full and reports the layer with other
/// pixels' stamp. At every step the window's frame with its layer (if any) is the frame a
/// backend without the switch paints.
///
/// Mutations: without the clip added to the damage at a switch-over the window's frame
/// keeps (or lacks) the picture under the layer; without the forced full frame after a
/// release an idle window shows no picture until something changes.
#[test]
fn a_frame_that_stops_or_starts_promoting_repaints_the_layers_clip_once() {
    let mut w = window();
    let mut layered = CpuBackend::new();
    layered.promote_for_present = true;
    let mut plain = CpuBackend::new();
    let same = |w: &Window, layered: &mut CpuBackend, plain: &mut CpuBackend, what: &str| {
        let below = w.frame(layered);
        let wanted = w.frame(plain);
        assert_eq!(differing(&shown(&below, layered), &wanted), 0, "{what}");
    };

    same(&w, &mut layered, &mut plain, "the first frame");
    let first = layer_of(&layered, &w, "the first frame");

    // Off: the next frame that has anything to do (a zoom step) paints the picture into the
    // window's frame, under the whole clip, and reports no layer.
    layered.promote_for_present = false;
    w.write(zoom(-20.0, -10.0, 1.25));
    same(&w, &mut layered, &mut plain, "the promotion stops");
    assert!(layered.last_present_layer.is_none());
    assert!(
        painted_area(&layered) >= 300.0 * 200.0,
        "the clip is repainted: {:?}",
        layered.last_frame_damage
    );

    // On again: the clip is repainted without the picture, and the layer is back.
    layered.promote_for_present = true;
    w.write(zoom(-30.0, -10.0, 1.25));
    same(&w, &mut layered, &mut plain, "the promotion starts again");
    let again = layer_of(&layered, &w, "the promotion starts again");
    assert_eq!(again.clip, first.clip);
    assert!(painted_area(&layered) >= 300.0 * 200.0, "{:?}", layered.last_frame_damage);

    // The kept surfaces are released (memory pressure, a window in the Dock).
    layered.release_caches();
    assert!(layered.last_present_layer.is_none(), "no surface, no layer");
    same(&w, &mut layered, &mut plain, "after a release");
    assert_eq!(layered.last_frame_damage, FrameDamage::Full, "nothing changed, and the frame is painted");
    let after = layer_of(&layered, &w, "after a release");
    assert_ne!(after.generation, again.generation, "the surface was painted again");
}
