//! A value written to a node INSIDE a `VirtualView` is painted by the next CPU frame.
//!
//! The map editor zooms its map by writing `translate() scale()` to the picture of the view -
//! a node of the view's child DOM that carries an identity `transform` at build time - with
//! `set_css_property`: a value under the node's key, no display list, no layout, no render of
//! the view. A node of a view binds its key in the CHILD's GPU value cache, and the CPU
//! frame read the root's alone: the picture stayed unscaled until the view rendered again
//! (WebRender reads every DOM's cache). The frame's values - and the damage that tells it
//! to paint - are now those of every DOM of the window, the lists of the views included.
//!
//! The reference here is HAND-COMPUTED: a 40 x 30 box in the child at (40, 30), scaled by
//! two about its centre and moved by (40, 20), is the 80 x 60 box at (60, 35) of the child; and
//! the frame after each write is compared with a frame a fresh backend paints in full, which
//! is what the incremental frame has to equal.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::{color::ColorU, FloatValue, PercentageValue, PixelValue},
    property::CssProperty,
    style::{
        background::{StyleBackgroundContent, StyleBackgroundContentVec},
        StyleOpacity, StyleTransform, StyleTransformScale2D, StyleTransformTranslate2D,
    },
};
use rust_fontconfig::FcFontCache;

use crate::cpu_backend::CpuBackend;
use crate::{
    callbacks::ExternalSystemCallbacks,
    cpurender::AzulPixmap,
    overlay::{ContentChange, ContentDirtyTier},
    window::{FrameDamage, LayoutWindow},
    window_state::FullWindowState,
};

/// The view's child: a red picture (40 x 30 at (40, 30)) that carries an IDENTITY transform at
/// build time, and a blue veil (50 x 50 at (220, 120)) at opacity 0.8.
extern "C" fn page(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let dom = Dom::create_div()
        .with_child(Dom::create_div().with_class("picture".into()).with_css(
            "position: absolute; left: 40px; top: 30px; width: 40px; height: 30px; \
             background: #ff0000; transform: translate(0px, 0px) scale(1);",
        ))
        .with_child(Dom::create_div().with_class("veil".into()).with_css(
            "position: absolute; left: 220px; top: 120px; width: 50px; height: 50px; \
             background: #0000ff; opacity: 0.8;",
        ));
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.get_logical_size());
    VirtualViewReturn::with_dom(dom, rect, rect)
}

/// A 400 x 300 window with the view at (50, 40), 300 x 200.
struct Zoomable {
    lw: LayoutWindow,
    resources: RendererResources,
    child: DomId,
    picture: NodeId,
    veil: NodeId,
    /// Where the child's (0, 0) is on screen: the view's corner plus the pan.
    origin: (f32, f32),
}

fn node_with_class(lw: &LayoutWindow, dom: DomId, class: &str) -> NodeId {
    let nodes = lw.layout_results[&dom].styled_dom.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| nodes[*n].has_class(class))
        .unwrap_or_else(|| panic!("a node of class {class}"))
}

/// The page, its view panned to `pan` (the materialized window's origin) before the first
/// frame when it is given.
fn zoomable(pan: Option<(f32, f32)>) -> Zoomable {
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
    let mut origin = (50.0, 40.0);
    if let Some((px, py)) = pan {
        lw.apply_virtual_view_geometry(
            host,
            view,
            Some(LogicalRect::new(
                LogicalPosition::new(px, py),
                LogicalSize::new(300.0, 200.0),
            )),
            None,
        );
        origin = (50.0 + px, 40.0 + py);
    }
    let picture = node_with_class(&lw, child, "picture");
    let veil = node_with_class(&lw, child, "veil");
    Zoomable {
        lw,
        resources,
        child,
        picture,
        veil,
        origin,
    }
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

fn opacity(percent: f32) -> CssProperty {
    CssProperty::opacity(StyleOpacity {
        inner: PercentageValue::new(percent),
    })
}

impl Zoomable {
    /// The write `set_css_property` makes, to a node of the view's DOM.
    fn write(&mut self, node: NodeId, prop: CssProperty) -> ContentDirtyTier {
        self.lw
            .apply_content_change(ContentChange::NodeCss {
                dom_id: self.child,
                node_id: node,
                props: vec![prop],
                override_only: false,
            })
            .tier
    }

    fn frame(&self, backend: &mut CpuBackend) -> AzulPixmap {
        backend.render_frame(&self.lw, &self.resources, 400.0, 300.0, 1.0);
        backend
            .last_frame
            .as_ref()
            .expect("the backend painted a frame")
            .clone_pixmap()
    }

    /// The inclusive pixel box `(x0, y0, x1, y1)` of the child's box at `(x, y)`, `w` x `h`
    /// in the child's space, on screen.
    fn on_screen(&self, (x, y, w, h): (f32, f32, f32, f32)) -> (u32, u32, u32, u32) {
        (
            (self.origin.0 + x) as u32,
            (self.origin.1 + y) as u32,
            (self.origin.0 + x + w) as u32 - 1,
            (self.origin.1 + y + h) as u32 - 1,
        )
    }

    /// The veil's centre pixel.
    fn veil_centre(&self, frame: &AzulPixmap) -> [u8; 3] {
        let p = pixel(frame, (self.origin.0 + 245.0) as u32, (self.origin.1 + 145.0) as u32);
        [p[0], p[1], p[2]]
    }
}

fn pixel(frame: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * frame.width() + x) * 4) as usize;
    let d = frame.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]]
}

/// The inclusive box of the pixels of `frame` that are red - the picture's edge pixels, where
/// a scaled surface is resampled, included: red by a clear margin over the other channels.
fn red_box(frame: &AzulPixmap) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let p = pixel(frame, x, y);
            if p[0] > 150 && p[0].saturating_sub(p[1].max(p[2])) > 40 {
                found = Some(match found {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }
    found
}

fn near(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 4)
}

/// The pixels where two frames differ by more than a rounding.
fn differing(a: &AzulPixmap, b: &AzulPixmap) -> Vec<(u32, u32)> {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let mut out = Vec::new();
    for y in 0..a.height() {
        for x in 0..a.width() {
            let (p, q) = (pixel(a, x, y), pixel(b, x, y));
            if (0..4).any(|c| p[c].abs_diff(q[c]) > 1) {
                out.push((x, y));
            }
        }
    }
    out
}

/// The frame an incremental backend paints is the frame a fresh backend paints in full: the
/// damage of the write covers every pixel the write changed.
fn assert_the_frame_is_what_a_full_paint_gives(z: &Zoomable, incremental: &AzulPixmap, what: &str) {
    let fresh = z.frame(&mut CpuBackend::new());
    let wrong = differing(incremental, &fresh);
    assert!(
        wrong.is_empty(),
        "{what}: the incremental frame differs from a full paint in {} pixels, first at {:?}",
        wrong.len(),
        wrong.first()
    );
}

#[test]
fn a_transform_written_inside_a_view_is_painted_by_the_next_cpu_frame() {
    let mut z = zoomable(None);
    let mut backend = CpuBackend::new();

    // Before the write: the identity.
    let first = z.frame(&mut backend);
    assert_eq!(red_box(&first), Some(z.on_screen((40.0, 30.0, 40.0, 30.0))), "the picture, unscaled");

    // translate(40px, 20px) scale(2) about the picture's centre (20, 15): its box is
    // 80 x 60 at (60, 35) of the view's content.
    let tier = z.write(z.picture, zoom(40.0, 20.0, 2.0));
    assert_eq!(tier, ContentDirtyTier::Paint, "a value: no rebuild of the list");
    z.lw.frame_report.reset_counters();
    let after = z.frame(&mut backend);
    assert_eq!(
        red_box(&after),
        Some(z.on_screen((60.0, 35.0, 80.0, 60.0))),
        "the next frame draws the picture scaled and moved"
    );
    assert_the_frame_is_what_a_full_paint_gives(&z, &after, "after the first write");
    match &backend.last_frame_damage {
        FrameDamage::Rects(rects) => {
            assert!(!rects.is_empty());
            for r in rects {
                assert!(
                    r.origin.x >= 49.0
                        && r.origin.y >= 39.0
                        && r.origin.x + r.size.width <= 351.0
                        && r.origin.y + r.size.height <= 241.0,
                    "the repaint stays inside the view's box: {r:?}"
                );
            }
        }
        other => panic!("an incremental repaint of the view, not {other:?}"),
    }

    // A second write REPLACES the first: it does not stack on it.
    z.write(z.picture, zoom(100.0, 50.0, 2.0));
    let replaced = z.frame(&mut backend);
    assert_eq!(
        red_box(&replaced),
        Some(z.on_screen((120.0, 65.0, 80.0, 60.0))),
        "translate(100px, 50px) scale(2) about the centre: 80 x 60 at (120, 65)"
    );
    assert_the_frame_is_what_a_full_paint_gives(&z, &replaced, "after the second write");

    // And back to the identity: the picture is where it was built.
    z.write(z.picture, zoom(0.0, 0.0, 1.0));
    let back = z.frame(&mut backend);
    assert_eq!(red_box(&back), red_box(&first));
    assert_the_frame_is_what_a_full_paint_gives(&z, &back, "back at the identity");

    // The writes asked for no list and no layout.
    assert_eq!(z.lw.frame_report.dl_rebuilds, 0, "no display list rebuilt");
    assert_eq!(z.lw.frame_report.layout_passes, 0, "no layout");

    // Nothing changes any more: the frame is idle.
    let _ = z.frame(&mut backend);
    assert_eq!(backend.last_frame_damage, FrameDamage::None, "a settled window paints nothing");
}

#[test]
fn an_opacity_written_inside_a_view_is_painted_by_the_next_cpu_frame() {
    let mut z = zoomable(None);
    let mut backend = CpuBackend::new();
    let first = z.frame(&mut backend);
    // Blue at 0.8 over white.
    assert!(near(z.veil_centre(&first), [51, 51, 255]), "{:?}", z.veil_centre(&first));

    let tier = z.write(z.veil, opacity(40.0));
    assert_eq!(tier, ContentDirtyTier::Paint, "a value: no rebuild of the list");
    let faded = z.frame(&mut backend);
    assert!(
        near(z.veil_centre(&faded), [153, 153, 255]),
        "blue at 0.4 over white, got {:?}",
        z.veil_centre(&faded)
    );
    assert_the_frame_is_what_a_full_paint_gives(&z, &faded, "after the first opacity");

    // The second write replaces: 0.6, not 0.4 x 0.6.
    z.write(z.veil, opacity(60.0));
    let again = z.frame(&mut backend);
    assert!(
        near(z.veil_centre(&again), [102, 102, 255]),
        "blue at 0.6 over white, got {:?}",
        z.veil_centre(&again)
    );
    assert_the_frame_is_what_a_full_paint_gives(&z, &again, "after the second opacity");
}

/// The view's content is painted at the item's origin PLUS its content offset: the damage of a
/// value inside a panned view has to land there too (a sign slip repaints a neighbour's
/// pixels and leaves the picture's old place standing).
#[test]
fn a_value_written_inside_a_panned_view_is_painted_where_the_view_shows_it() {
    let mut z = zoomable(Some((-20.0, -15.0)));
    assert_eq!(z.origin, (30.0, 25.0));
    let mut backend = CpuBackend::new();
    let first = z.frame(&mut backend);
    assert_eq!(red_box(&first), Some(z.on_screen((40.0, 30.0, 40.0, 30.0))), "the panned picture");

    z.write(z.picture, zoom(40.0, 20.0, 2.0));
    let after = z.frame(&mut backend);
    assert_eq!(
        red_box(&after),
        Some(z.on_screen((60.0, 35.0, 80.0, 60.0))),
        "scaled and moved, in the panned content"
    );
    assert_the_frame_is_what_a_full_paint_gives(&z, &after, "a panned view after a write");

    z.write(z.veil, opacity(40.0));
    let faded = z.frame(&mut backend);
    assert!(near(z.veil_centre(&faded), [153, 153, 255]), "{:?}", z.veil_centre(&faded));
    assert_the_frame_is_what_a_full_paint_gives(&z, &faded, "a panned view after an opacity");
}

fn background(r: u8, g: u8, b: u8) -> CssProperty {
    let layers: StyleBackgroundContentVec =
        vec![StyleBackgroundContent::Color(ColorU { r, g, b, a: 255 })].into();
    CssProperty::background_content(layers)
}

impl Zoomable {
    /// The colour of the pixel at `(x, y)` of the child's space, on screen.
    fn child_pixel(&self, frame: &AzulPixmap, x: f32, y: f32) -> [u8; 3] {
        let p = pixel(frame, (self.origin.0 + x) as u32, (self.origin.1 + y) as u32);
        [p[0], p[1], p[2]]
    }
}

/// A node of a panned view that changes COLOUR - a tile arriving, a label under the pointer -
/// is a change in the view's own list: the parent's item stays the same, so the frame finds
/// it by diffing the two lists of the view, and the damage has to land where the panned view
/// shows the node. The view's content is drawn at the item's origin PLUS its content offset;
/// the damage was placed at the origin MINUS it, a neighbour's pixels away, and the picture
/// stayed red on screen while its list said green.
#[test]
fn a_colour_written_inside_a_panned_view_is_painted_where_the_view_shows_it() {
    let mut z = zoomable(Some((-20.0, -15.0)));
    assert_eq!(z.origin, (30.0, 25.0));
    let mut backend = CpuBackend::new();
    let first = z.frame(&mut backend);
    assert!(
        near(z.child_pixel(&first, 60.0, 45.0), [255, 0, 0]),
        "premise: the picture is red, got {:?}",
        z.child_pixel(&first, 60.0, 45.0)
    );

    z.write(z.picture, background(0, 255, 0));
    let after = z.frame(&mut backend);
    assert!(
        near(z.child_pixel(&after, 60.0, 45.0), [0, 255, 0]),
        "the picture turned green where the panned view shows it, got {:?}",
        z.child_pixel(&after, 60.0, 45.0)
    );
    assert_the_frame_is_what_a_full_paint_gives(&z, &after, "a recolour in a panned view");
    match &backend.last_frame_damage {
        FrameDamage::Rects(rects) => {
            let area: f32 = rects.iter().map(|r| r.size.width * r.size.height).sum();
            assert!(area < 300.0 * 200.0 * 0.5, "the picture's box, not the view: {rects:?}");
        }
        other => panic!("an incremental repaint of the picture, not {other:?}"),
    }
}

/// The same change under a frame whose live matrix is not the one the list was built with: the
/// picture of the map is zoomed by a write of its `transform` and then a tile arrives (its
/// colour changes). The frame paints the picture through the zoom, so does the damage.
#[test]
fn a_colour_written_under_a_zoomed_frame_inside_a_view_is_painted_where_the_zoom_shows_it() {
    let mut z = zoomable(None);
    let mut backend = CpuBackend::new();
    let _ = z.frame(&mut backend);
    z.write(z.picture, zoom(40.0, 20.0, 2.0));
    let zoomed = z.frame(&mut backend);
    assert_eq!(red_box(&zoomed), Some(z.on_screen((60.0, 35.0, 80.0, 60.0))), "premise: zoomed");

    z.write(z.picture, background(0, 255, 0));
    let after = z.frame(&mut backend);
    // The centre of the zoomed picture, (100, 65) of the view's content.
    assert!(
        near(z.child_pixel(&after, 100.0, 65.0), [0, 255, 0]),
        "the zoomed picture turned green, got {:?}",
        z.child_pixel(&after, 100.0, 65.0)
    );
    assert_the_frame_is_what_a_full_paint_gives(&z, &after, "a recolour under a zoomed frame");
}

/// What the CPU frame just painted, counted ([`crate::cpu_counters`]).
fn last_frame(z: &Zoomable) -> crate::cpu_counters::CpuFrameCounters {
    z.lw.cpu_frame_counters().last
}

#[test]
fn a_zoom_step_is_counted_and_a_settled_frame_counts_nothing() {
    let mut z = zoomable(None);
    let mut backend = CpuBackend::new();
    let _ = z.frame(&mut backend);
    let first = last_frame(&z);
    assert_eq!(first.groups_opened, 0, "the page has no transform: no group surface");
    assert_eq!(first.px_cleared, 0, "a first frame is a full paint, not a clear of damage");
    assert!(first.items_painted > 0, "and it paints the page: {first:?}");

    // One zoom step: one reference frame opens one group surface, the picture's 40 x 30 box
    // scaled to 80 x 60 px, composited once; only the view is cleared, and the items of the
    // view are painted once.
    z.write(z.picture, zoom(40.0, 20.0, 2.0));
    let _ = z.frame(&mut backend);
    let step = last_frame(&z);
    assert_eq!(step.groups_opened, 1, "the zoomed frame is one group: {step:?}");
    assert_eq!(step.group_px, 40 * 30, "the group is the picture's own box: {step:?}");
    assert_eq!(step.px_blitted, 80 * 60, "the composite blits the scaled box once: {step:?}");
    assert!(
        step.px_cleared <= 300 * 200,
        "the clear is inside the view's box, not the window: {step:?}"
    );
    // The view's items, once each - less those the zoom moves out of the view (the walk culls
    // them: only what can land is painted).
    assert!(
        step.items_painted <= first.items_painted && step.items_painted + 3 >= first.items_painted,
        "the view's items, once: {step:?} against {first:?}"
    );
    assert_eq!(step.retained_hits, 0, "nothing is retained yet");

    // A settled window counts nothing.
    let _ = z.frame(&mut backend);
    assert_eq!(
        last_frame(&z),
        crate::cpu_counters::CpuFrameCounters::default(),
        "an idle frame walks, paints, clears and blits nothing"
    );
}

/// A1-11 (`audit/A1.md`; A10-08): a frame painted at another scale factor than the frame
/// before is painted in full, and keeps no group surface of the old factor. The backend is
/// told that its last frame was at a factor of 2; the next frame, at 1, must not be a repaint
/// of parts (the list did not change: the diff alone finds nothing and the frame would be
/// skipped, showing the old scale's pixels).
/// Mutation: `dpi_changed` left out of `frame_may_reuse_previous_pixels` skips the frame.
#[test]
fn a1_11_a_frame_at_another_scale_factor_is_painted_in_full() {
    let z = zoomable(None);
    let mut backend = CpuBackend::new();
    let first = z.frame(&mut backend);
    let _ = z.frame(&mut backend);
    assert!(matches!(backend.last_frame_damage, crate::window::FrameDamage::None), "premise: an idle frame");
    backend.last_dpi_factor = Some(2.0);
    let again = z.frame(&mut backend);
    assert!(
        matches!(backend.last_frame_damage, crate::window::FrameDamage::Full),
        "another factor: a full frame, got {:?}",
        backend.last_frame_damage
    );
    assert_eq!(backend.last_dpi_factor, Some(1.0));
    assert!(backend.retained_frames.is_empty(), "no surface of the old factor is kept");
    assert!(first.data() == again.data(), "and it is the frame a first paint gives");
}

/// A1-16: the GPU values a frame paints with are moved into its render state and come back:
/// after a zoom step the backend holds the picture's matrix again (the next frame diffs
/// against it), and that next frame, with nothing written, is idle.
/// Mutation: the values not moved back leaves `previous_gpu_values` empty, and the next frame
/// sees every value as changed.
#[test]
fn a1_16_the_values_a_frame_painted_with_are_the_next_frames_previous_values() {
    let mut z = zoomable(None);
    let mut backend = CpuBackend::new();
    let _ = z.frame(&mut backend);
    z.write(z.picture, zoom(40.0, 20.0, 2.0));
    let _ = z.frame(&mut backend);
    assert!(
        !backend.previous_gpu_values.transforms.is_empty(),
        "the frame's values are the backend's again"
    );
    let _ = z.frame(&mut backend);
    assert!(matches!(backend.last_frame_damage, crate::window::FrameDamage::None), "nothing written: an idle frame");
}

/// A1 (merge follow-up 3): the frame `render_frame` paints is as large as
/// `present_geometry::frame_size_px` says, the function every shell sizes its buffer with
/// (A10): 400.2 x 300 logical px at a factor of 2 are 801 x 600 px (800.4 rounds up), not the
/// 800 a truncation gives.
/// Mutation: a size of its own in `render_frame` that truncates is 800 px wide.
#[test]
fn the_frame_is_as_large_as_the_shells_buffer_formula_says() {
    let z = zoomable(None);
    let mut backend = CpuBackend::new();
    backend.render_frame(&z.lw, &z.resources, 400.2, 300.0, 2.0);
    let expected = crate::window::present_geometry::frame_size_px(400.2, 300.0, 2.0);
    assert_eq!(expected, (801, 600), "premise: ceil(800.4) x 600");
    let frame = backend.last_frame.as_ref().expect("the backend painted a frame");
    assert_eq!((frame.width(), frame.height()), expected);
    assert_eq!(backend.last_frame_px, Some(expected));
}

/// A1-29 (merge follow-up 4): what the macOS shell gives back for a window nobody sees and
/// under memory pressure (`CpuBackend::release_caches`: the kept group surfaces, this thread's
/// kept halos, the compositor) costs the next frames work, never a pixel: an unchanged frame
/// after the release is idle (the frame on screen is kept), and a zoom step after it is the
/// frame a backend that never kept anything paints.
/// Mutation: a release that also drops `last_frame` or the previous display list turns the
/// idle frame into a full one.
#[test]
fn a_backend_that_released_its_caches_paints_the_same_frames() {
    let mut z = zoomable(None);
    let mut backend = CpuBackend::new();
    let _ = z.frame(&mut backend);
    z.write(z.picture, zoom(40.0, 20.0, 2.0));
    let _ = z.frame(&mut backend);
    backend.release_caches();
    assert!(backend.retained_frames.is_empty() && backend.compositor.is_none());
    let _ = z.frame(&mut backend);
    assert!(
        matches!(backend.last_frame_damage, crate::window::FrameDamage::None),
        "nothing changed: an idle frame, got {:?}",
        backend.last_frame_damage
    );
    z.write(z.picture, zoom(40.0, 20.0, 2.5));
    let after = z.frame(&mut backend);
    assert_the_frame_is_what_a_full_paint_gives(&z, &after, "a zoom step after a release");
}
