//! A2-cpu, signs and offsets on the CPU path.
//!
//! Two kinds of test, both without text (no font, no anti-aliasing to argue about):
//!
//! - hand-built display lists through the compositor's own functions, where a number can be
//!   computed by hand (which side a pan's strip is on, where a damage rect lands);
//! - a window with a `VirtualView` built like the map editor's (a picture root that carries the
//!   zoom's transform, an opaque ground around the view, turned boxes inside it), driven through
//!   pans, value writes and re-renders: after every step the frame the damage path painted must
//!   be the frame a fresh backend paints, pixel for pixel.
//!
//! The findings are in `audit/A2-cpu.md`.
#![cfg(all(feature = "cpurender", feature = "text_layout"))]

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    FastBTreeSet,
};
use azul_css::props::{
    basic::{color::ColorU, AngleValue, FloatValue, FontRef, PercentageValue, PixelValue},
    property::CssProperty,
    style::{StyleOpacity, StyleTransform, StyleTransformScale2D, StyleTransformTranslate2D},
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    cpu_backend::CpuBackend,
    cpurender::{
        collect_virtual_view_shifts, execute_virtual_view_shift, render_display_list_damaged,
        AzulPixmap, CpuRenderState, ScrollOffsetMap,
    },
    glyph_cache::GlyphCache,
    overlay::{ContentChange, ContentDirtyTier},
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect},
    text3::cache::FontManager,
    window::{FrameDamage, LayoutWindow},
    window_state::FullWindowState,
};

// ---------------------------------------------------------------------------------------------
// Shared: comparing two frames
// ---------------------------------------------------------------------------------------------

/// How many pixels of two frames differ in colour, and the first one that does.
fn differing(a: &AzulPixmap, b: &AzulPixmap) -> (usize, Option<(u32, u32)>) {
    differing_by_more_than(a, b, 0)
}

/// How many pixels of two frames differ by more than `levels` in a colour channel, and the
/// first one that does.
fn differing_by_more_than(
    a: &AzulPixmap,
    b: &AzulPixmap,
    levels: u8,
) -> (usize, Option<(u32, u32)>) {
    assert_eq!(
        (a.width(), a.height()),
        (b.width(), b.height()),
        "harness: two frames of one window"
    );
    let (da, db) = (a.data(), b.data());
    let mut count = 0usize;
    let mut first = None;
    for i in (0..da.len().min(db.len())).step_by(4) {
        if (0..3).any(|c| da[i + c].abs_diff(db[i + c]) > levels) {
            count += 1;
            if first.is_none() {
                let px = (i / 4) as u32;
                first = Some((px % a.width(), px / a.width()));
            }
        }
    }
    (count, first)
}

/// Do two rects agree to a hundredth of a pixel?
fn same(a: &LogicalRect, b: &LogicalRect) -> bool {
    (a.origin.x - b.origin.x).abs() < 0.01
        && (a.origin.y - b.origin.y).abs() < 0.01
        && (a.size.width - b.size.width).abs() < 0.01
        && (a.size.height - b.size.height).abs() < 0.01
}

fn lr(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

/// The two sets of rects hold the same rects, in any order.
fn same_rects(got: &[LogicalRect], want: &[LogicalRect]) -> bool {
    got.len() == want.len()
        && want.iter().all(|w| got.iter().any(|g| same(g, w)))
        && got.iter().all(|g| want.iter().any(|w| same(g, w)))
}

// ---------------------------------------------------------------------------------------------
// Hand-built lists: a view's pan (A2-cpu-11, `execute_virtual_view_shift`)
// ---------------------------------------------------------------------------------------------

/// The window of the hand-built lists, logical px.
const LIST_W: f32 = 400.0;
const LIST_H: f32 = 300.0;
/// The view's box in the parent list: (48, 40), 304 x 200. Whole device pixels at 1, 1.25, 1.5, 2.
const VIEW: (f32, f32, f32, f32) = (48.0, 40.0, 304.0, 200.0);
/// The view's child DOM.
const CHILD: DomId = DomId { inner: 1 };

fn wlr(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
    lr(x, y, w, h).into()
}

fn rect(x: f32, y: f32, w: f32, h: f32, [r, g, b]: [u8; 3]) -> DisplayListItem {
    DisplayListItem::Rect {
        bounds: wlr(x, y, w, h),
        color: ColorU { r, g, b, a: 255 },
        border_radius: BorderRadius::default(),
    }
}

fn list(items: Vec<DisplayListItem>) -> DisplayList {
    DisplayList {
        items,
        ..Default::default()
    }
}

/// The parent list: a grey page and the view, its content at `offset`.
fn parent_list(offset: (f32, f32)) -> DisplayList {
    list(vec![
        rect(0.0, 0.0, LIST_W, LIST_H, [232, 232, 232]),
        DisplayListItem::VirtualView {
            child_dom_id: CHILD,
            bounds: wlr(VIEW.0, VIEW.1, VIEW.2, VIEW.3),
            clip_rect: wlr(VIEW.0, VIEW.1, VIEW.2, VIEW.3),
            content_offset: LogicalPosition::new(offset.0, offset.1),
        },
    ])
}

/// The view's content: an opaque ground 256 px around the view (what the map editor draws, so a
/// pan inside it may move pixels) and a grid of boxes of 48 colours over it, so that a move by
/// the wrong amount or to the wrong side shows. Every edge is a multiple of 4 px: a whole device
/// pixel at 1.25, 1.5 and 2 as well (no edge on a half pixel, whose rounding is not the point).
fn child_list() -> Arc<DisplayList> {
    let mut items = vec![rect(-256.0, -256.0, VIEW.2 + 512.0, VIEW.3 + 512.0, [47, 49, 54])];
    for j in 0..6u8 {
        for i in 0..8u8 {
            let colour = [40 + 25 * i, 250 - 35 * j, 30 + 9 * (i + 3 * j)];
            items.push(rect(
                f32::from(i) * 56.0 - 88.0,
                f32::from(j) * 52.0 - 68.0,
                36.0,
                32.0,
                colour,
            ));
        }
    }
    Arc::new(list(items))
}

fn views(child: &Arc<DisplayList>) -> BTreeMap<DomId, Arc<DisplayList>> {
    let mut views = BTreeMap::new();
    views.insert(CHILD, Arc::clone(child));
    views
}

fn font_manager() -> FontManager<FontRef> {
    FontManager::new(FcFontCache::default()).expect("FontManager::new")
}

fn list_pixmap(dpi: f32) -> AzulPixmap {
    let mut out = AzulPixmap::new((LIST_W * dpi).ceil() as u32, (LIST_H * dpi).ceil() as u32)
        .expect("a pixmap");
    out.fill(255, 255, 255, 255);
    out
}

/// Repaints `damage` of `parent` (and the view in it) into `out`.
fn repaint(
    parent: &DisplayList,
    child: &Arc<DisplayList>,
    out: &mut AzulPixmap,
    dpi: f32,
    damage: &[LogicalRect],
) {
    let state = CpuRenderState::new(ScrollOffsetMap::new())
        .with_virtual_view_display_lists(views(child));
    let mut glyphs = GlyphCache::new();
    render_display_list_damaged(
        parent,
        out,
        dpi,
        &RendererResources::default(),
        &font_manager(),
        &mut glyphs,
        &state,
        damage,
    )
    .expect("the damage repaints");
}

/// `parent` painted from nothing: the whole window as one damage rect.
fn fresh_list_frame(parent: &DisplayList, child: &Arc<DisplayList>, dpi: f32) -> AzulPixmap {
    let mut out = list_pixmap(dpi);
    repaint(parent, child, &mut out, dpi, &[lr(0.0, 0.0, LIST_W, LIST_H)]);
    out
}

/// The frame of a pan from `from` to `to` as a CPU frame makes it: the frame before, the shift
/// of the view (its pixels moved or its box damaged), the damage repainted. With what the shift
/// answered: its damage and what it asks to present on top.
fn panned(
    from: (f32, f32),
    to: (f32, f32),
    dpi: f32,
) -> (AzulPixmap, Vec<LogicalRect>, Vec<LogicalRect>) {
    let child = child_list();
    let (old, new) = (parent_list(from), parent_list(to));
    let mut frame = fresh_list_frame(&old, &child, dpi);
    let none = ScrollOffsetMap::new();
    let shifts = collect_virtual_view_shifts(&old, &new, &none, &none);
    assert_eq!(shifts.len(), 1, "harness: the view only scrolled, from {from:?} to {to:?}");
    let out = execute_virtual_view_shift(&mut frame, &new, &views(&child), &shifts[0], dpi, &none);
    if !out.damage.is_empty() {
        repaint(&new, &child, &mut frame, dpi, &out.damage);
    }
    (frame, out.damage, out.present_extra)
}

/// A2-cpu-11. The strip a pan repaints is on the side the content came FROM, for all eight
/// directions, at a zero, a negative and a positive content offset.
///
/// Hand-computed at dpi 1, view box (48, 40) 304 x 200. `content_offset` grows by (12, 0): the
/// content moves 12 px RIGHT, the strip is the view's LEFT edge, 12 + 1 px wide (the mover's
/// one pixel of over-cover): (48, 40) 13 x 200. By (-12, 0): the right edge, x = 48 + 304 - 13 =
/// 339. By (0, 7): content down, the top strip (48, 40) 304 x 8. By (0, -7): the bottom strip,
/// y = 40 + 200 - 8 = 232. A diagonal gives both strips. The whole box is presented on top.
///
/// mutation: swap the two arms of `x = if px_dx > 0 { .. }` in `scroll_shift_region_impl`, or
/// negate `delta` in `collect_virtual_view_shifts` (`ooff - noff` -> `noff - ooff`): fails here.
#[test]
fn a2_cpu_11_a_pans_strip_is_on_the_side_the_content_came_from_in_all_eight_directions() {
    let (vx, vy, vw, vh) = VIEW;
    let left = lr(vx, vy, 13.0, vh);
    let right = lr(vx + vw - 13.0, vy, 13.0, vh);
    let top = lr(vx, vy, vw, 8.0);
    let bottom = lr(vx, vy + vh - 8.0, vw, 8.0);
    let cases: [((f32, f32), Vec<LogicalRect>); 8] = [
        ((12.0, 0.0), vec![left]),
        ((-12.0, 0.0), vec![right]),
        ((0.0, 7.0), vec![top]),
        ((0.0, -7.0), vec![bottom]),
        ((12.0, 7.0), vec![left, top]),
        ((12.0, -7.0), vec![left, bottom]),
        ((-12.0, 7.0), vec![right, top]),
        ((-12.0, -7.0), vec![right, bottom]),
    ];
    for base in [(0.0_f32, 0.0_f32), (-100.0, 60.0), (100.0, -60.0)] {
        for (d, strips) in &cases {
            let to = (base.0 + d.0, base.1 + d.1);
            let (frame, damage, present_extra) = panned(base, to, 1.0);
            assert!(
                same_rects(&damage, strips),
                "pan by {d:?} from {base:?}: the strips are {damage:?}, expected {strips:?}"
            );
            assert!(
                same_rects(&present_extra, &[lr(vx, vy, vw, vh)]),
                "pan by {d:?} from {base:?}: the moved box is presented: {present_extra:?}"
            );
            let (n, first) =
                differing(&frame, &fresh_list_frame(&parent_list(to), &child_list(), 1.0));
            assert_eq!(
                n, 0,
                "pan by {d:?} from {base:?}: the moved frame is not a fresh render's, first at \
                 {first:?}"
            );
        }
    }
}

/// A2-cpu-11. The same eight directions at device scales 1.25, 1.5 and 2 (the deltas are whole
/// device pixels at each), a pan that is no whole device pixel (the box is repainted, nothing
/// moves) and a pan further than the view is wide (nothing can be kept): the frame is a fresh
/// render's every time.
#[test]
fn a2_cpu_11_a_panned_frame_is_a_fresh_renders_at_every_scale_and_for_every_kind_of_pan() {
    let (vx, vy, vw, vh) = VIEW;
    for dpi in [1.0_f32, 1.25, 1.5, 2.0] {
        for base in [(0.0_f32, 0.0_f32), (-96.0, 64.0), (96.0, -64.0)] {
            for d in [
                (12.0_f32, 0.0_f32),
                (-12.0, 0.0),
                (0.0, 8.0),
                (0.0, -8.0),
                (12.0, 8.0),
                (12.0, -8.0),
                (-12.0, 8.0),
                (-12.0, -8.0),
                // further than the view is wide, and than it is high
                (352.0, 0.0),
                (0.0, -240.0),
            ] {
                let to = (base.0 + d.0, base.1 + d.1);
                let (frame, damage, _) = panned(base, to, dpi);
                let (n, first) =
                    differing(&frame, &fresh_list_frame(&parent_list(to), &child_list(), dpi));
                assert_eq!(
                    n, 0,
                    "dpi {dpi}, pan by {d:?} from {base:?} (damage {damage:?}): first differing \
                     pixel {first:?}"
                );
            }
        }
    }
    // A third of a pixel: the content would land between pixels. No move, the box whole, and
    // nothing presented on top of the damage.
    let (frame, damage, present_extra) = panned((0.0, 0.0), (0.4, 0.0), 1.0);
    assert!(
        same_rects(&damage, &[lr(vx, vy, vw, vh)]),
        "a pan by 0.4 px repaints the view's box: {damage:?}"
    );
    assert!(present_extra.is_empty(), "and moves nothing: {present_extra:?}");
    let (n, first) =
        differing(&frame, &fresh_list_frame(&parent_list((0.4, 0.0)), &child_list(), 1.0));
    assert_eq!(n, 0, "a pan by 0.4 px: first differing pixel {first:?}");
}

// ---------------------------------------------------------------------------------------------
// A window with a view built like the map editor's
// ---------------------------------------------------------------------------------------------

const WIN_W: f32 = 400.0;
const WIN_H: f32 = 300.0;

/// What the view's callback draws.
struct Scene {
    /// Bumped by an edit: the next render of the view draws another picture.
    edits: Arc<AtomicUsize>,
    /// Whether the picture has the opaque ground of a map around the view.
    ground: bool,
}

fn abs_box(x: f32, y: f32, w: f32, h: f32, rest: &str) -> Dom {
    Dom::create_div().with_css(&format!(
        "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; {rest}"
    ))
}

/// The view's DOM: a `picture` that carries the identity transform at build time (the zoom
/// writes it), around its top left corner; in it the ground, twelve boxes, a box an edit moves
/// and recolours, a `turned` box with a box inside (a frame inside the picture's frame) and a
/// `veil` at opacity 0.8. Every edge is a multiple of 4 px (a whole device pixel at 1.25, 1.5
/// and 2).
extern "C" fn scene(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let (edit, ground) = match data.downcast_ref::<Scene>() {
        Some(scene) => (scene.edits.load(Ordering::SeqCst), scene.ground),
        None => (0, true),
    };
    let size = info.get_bounds().get_logical_size();
    let mut picture = abs_box(
        0.0,
        0.0,
        size.width,
        size.height,
        "transform-origin: 0px 0px; transform: translate(0px, 0px) scale(1);",
    )
    .with_class("picture".into());
    if ground {
        picture = picture.with_child(abs_box(
            -256.0,
            -256.0,
            size.width + 512.0,
            size.height + 512.0,
            "background: #2f3136;",
        ));
    }
    for i in 0..12usize {
        let colour = format!(
            "background: #{:02x}{:02x}{:02x};",
            40 + 17 * i,
            230 - 15 * i,
            (60 + 37 * i) % 256
        );
        picture = picture.with_child(abs_box(
            12.0 + 72.0 * (i % 4) as f32,
            8.0 + 48.0 * (i / 4) as f32,
            44.0,
            28.0,
            &colour,
        ));
    }
    picture = picture.with_child(abs_box(
        20.0 + 12.0 * (edit % 9) as f32,
        168.0,
        32.0,
        20.0,
        if edit % 2 == 0 { "background: #ff00ff;" } else { "background: #00ffff;" },
    ));
    picture = picture.with_child(
        abs_box(
            148.0,
            100.0,
            80.0,
            20.0,
            "background: #00aa55; transform-origin: 0px 0px; transform: rotate(30deg);",
        )
        .with_class("turned".into())
        .with_child(abs_box(8.0, 4.0, 40.0, 12.0, "background: #ffffff;")),
    );
    picture = picture.with_child(
        abs_box(220.0, 120.0, 48.0, 48.0, "background: #0000ff; opacity: 0.8;")
            .with_class("veil".into()),
    );
    let page = Dom::create_div().with_child(picture);
    let whole = LogicalRect::new(LogicalPosition::zero(), size);
    VirtualViewReturn::with_dom(page, whole, whole)
}

/// The window: a flat page, the view at (48, 40) 304 x 200 and - with `badge` - a box painted
/// OVER the view's bottom right corner that carries a translation (a value under a key).
fn host(scene_data: RefAny, badge: bool) -> Dom {
    let mut body = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(abs_box(0.0, 0.0, WIN_W, WIN_H, "background: #e8e8e8;"))
        .with_child(
            Dom::create_virtual_view(scene_data, VirtualViewCallback::create(scene)).with_css(
                "position: absolute; left: 48px; top: 40px; width: 304px; height: 200px;",
            ),
        );
    if badge {
        body = body.with_child(
            abs_box(
                320.0,
                220.0,
                60.0,
                40.0,
                "background: #ffcc00; transform: translate(0px, 0px);",
            )
            .with_class("badge".into()),
        );
    }
    body
}

/// A laid-out window, the backend that presents it frame after frame, and the handles of what
/// the tests move.
struct Stage {
    lw: LayoutWindow,
    backend: CpuBackend,
    view: NodeId,
    child: DomId,
    edits: Arc<AtomicUsize>,
    dpi: f32,
}

fn node_with_class(lw: &LayoutWindow, dom: DomId, class: &str) -> NodeId {
    let nodes = lw.layout_results[&dom].styled_dom.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| nodes[*n].has_class(class))
        .unwrap_or_else(|| panic!("harness: a node of class {class} in dom {}", dom.inner))
}

fn stage(dpi: u32, ground: bool, badge: bool) -> Stage {
    let edits = Arc::new(AtomicUsize::new(0));
    let mut dom = host(
        RefAny::new(Scene {
            edits: Arc::clone(&edits),
            ground,
        }),
        badge,
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIN_W, WIN_H);
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
    let (host_dom, view) = *lw
        .virtual_view_manager
        .all_view_keys()
        .first()
        .expect("harness: the page holds a view");
    assert_eq!(host_dom, DomId::ROOT_ID);
    let child = lw
        .virtual_view_manager
        .get_nested_dom_id(host_dom, view)
        .expect("harness: the view's child DOM");
    Stage {
        lw,
        backend: CpuBackend::new(),
        view,
        child,
        edits,
        dpi: dpi as f32 / 96.0,
    }
}

impl Stage {
    /// The next frame of the window's own backend: the damage path after the first.
    fn paint(&mut self) -> AzulPixmap {
        let _ = self.backend.render_frame(
            &self.lw,
            &RendererResources::default(),
            WIN_W,
            WIN_H,
            self.dpi,
        );
        self.backend
            .last_frame
            .as_ref()
            .expect("harness: a frame")
            .clone_pixmap()
    }

    /// The window as it stands, painted by a backend that holds no frame: what the screen must
    /// show, whatever path presented it.
    fn fresh(&self) -> AzulPixmap {
        let mut backend = CpuBackend::new();
        let _ = backend.render_frame(
            &self.lw,
            &RendererResources::default(),
            WIN_W,
            WIN_H,
            self.dpi,
        );
        backend
            .last_frame
            .as_ref()
            .expect("harness: a fresh frame")
            .clone_pixmap()
    }

    /// The editor's pan: the picture `(x, y)` px along (`CallbackInfo::update_virtual_view`).
    fn pan(&mut self, x: f32, y: f32) {
        self.lw.apply_virtual_view_geometry(
            DomId::ROOT_ID,
            self.view,
            Some(LogicalRect::new(
                LogicalPosition::new(x, y),
                LogicalSize::new(VIEW.2, VIEW.3),
            )),
            None,
        );
    }

    /// `CallbackInfo::set_css_property` of a value the display list binds: no rebuild.
    fn write(&mut self, dom: DomId, class: &str, prop: CssProperty) {
        let node = node_with_class(&self.lw, dom, class);
        let tier = self
            .lw
            .apply_content_change(ContentChange::NodeCss {
                dom_id: dom,
                node_id: node,
                props: vec![prop],
                override_only: false,
            })
            .tier;
        assert_eq!(
            tier,
            ContentDirtyTier::Paint,
            "harness: a value under the key of `{class}`, not a rebuild"
        );
    }

    /// The view renders again (its callback draws the next picture): the frame's drain of a
    /// queued re-render.
    fn edit(&mut self) {
        self.edits.fetch_add(1, Ordering::SeqCst);
        let mut updates = BTreeMap::new();
        let mut views = FastBTreeSet::new();
        views.insert(self.view);
        updates.insert(DomId::ROOT_ID, views);
        self.lw.queue_virtual_view_updates(updates);
        let window_state = self.lw.current_window_state.clone();
        let updated = self.lw.process_pending_virtual_view_updates(
            &window_state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
        );
        assert_eq!(updated.len(), 1, "harness: the view rendered again");
        self.child = self
            .lw
            .virtual_view_manager
            .get_nested_dom_id(DomId::ROOT_ID, self.view)
            .expect("harness: the view's child DOM after its render");
    }
}

/// `translate(tx, ty) scale(k)`: the editor's zoom (`write_picture_transform`).
fn zoom_css(tx: f32, ty: f32, k: f32) -> CssProperty {
    CssProperty::transform(
        vec![
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::px(tx),
                y: PixelValue::px(ty),
            }),
            StyleTransform::Scale(StyleTransformScale2D {
                x: FloatValue::new(k),
                y: FloatValue::new(k),
            }),
        ]
        .into(),
    )
}

/// `translate(tx, ty) rotate(degrees)`.
fn turn_css(tx: f32, ty: f32, degrees: f32) -> CssProperty {
    CssProperty::transform(
        vec![
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::px(tx),
                y: PixelValue::px(ty),
            }),
            StyleTransform::Rotate(AngleValue::deg(degrees)),
        ]
        .into(),
    )
}

fn move_css(tx: f32, ty: f32) -> CssProperty {
    CssProperty::transform(
        vec![StyleTransform::Translate(StyleTransformTranslate2D {
            x: PixelValue::px(tx),
            y: PixelValue::px(ty),
        })]
        .into(),
    )
}

fn opacity_css(percent: f32) -> CssProperty {
    CssProperty::opacity(StyleOpacity {
        inner: PercentageValue::new(percent),
    })
}

/// A2-cpu-13. A view's content offset is a whole device pixel of the FRAME, also at a scale that
/// three decimals do not hold.
///
/// Hand-computed at dpi 128 (a scale of 4/3; the frame is rendered at `128 / 96 = 1.3333334`,
/// `get_hidpi_factor()` reads 1.333). The window at (-300.3, -150.15): `-300.3 x 1.3333334 =
/// -400.4`, rounded -400, back `-400 / 1.3333334 = -299.99997`, in the frame `-400.0`. On the
/// 1.333 grid it was `round(-400.2999) / 1.333 = -300.075`, in the frame `-400.1`: a tenth of a
/// pixel off, and the next wheel step's delta (13.0032 device px) failed the whole-pixel gate of
/// `execute_virtual_view_shift`, so the view was repainted whole.
///
/// mutation: `self.frame_dpi_factor()` -> `self.current_window_state.size.get_hidpi_factor()
/// .inner.get()` in `virtual_view_content_offset` fails here.
#[test]
fn a2_cpu_13_a_views_content_offset_is_a_whole_device_pixel_of_the_frame_at_four_thirds() {
    let mut s = stage(128, true, false);
    let frame_scale = 128.0_f32 / 96.0;
    assert!(
        (s.lw.frame_dpi_factor() - frame_scale).abs() < 1e-6,
        "the frame's factor is dpi / 96: {}",
        s.lw.frame_dpi_factor()
    );
    s.pan(-300.3, -150.15);
    let at = s.lw.virtual_view_content_offset(DomId::ROOT_ID, s.view);
    assert!(
        (at.x * frame_scale + 400.0).abs() < 1e-3 && (at.y * frame_scale + 200.0).abs() < 1e-3,
        "(-300.3, -150.15) is device pixel (-400, -200) of the frame: {:?} is ({}, {})",
        at,
        at.x * frame_scale,
        at.y * frame_scale
    );
    for (x, y) in [(-290.1_f32, 7.7_f32), (12.4, -0.3), (239.7, 101.01), (0.2, 0.2)] {
        s.pan(x, y);
        let at = s.lw.virtual_view_content_offset(DomId::ROOT_ID, s.view);
        for device in [at.x * frame_scale, at.y * frame_scale] {
            assert!(
                (device - device.round()).abs() < 1e-3,
                "a window at ({x}, {y}): the offset {at:?} is {device} device px of the frame"
            );
        }
    }
}

/// A linear congruential generator: a failure names its seed and step, and reproduces.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }

    fn pick(&mut self, of: &[f32]) -> f32 {
        of[self.next() % of.len()]
    }
}

/// One random step of the editor's kind; what it did, for the failure message.
fn random_step(s: &mut Stage, rng: &mut Lcg, badge: bool) -> String {
    const PANS: [f32; 15] = [
        -239.0, -120.5, -37.0, -12.0, -1.0, -0.4, 0.0, 0.3, 1.0, 12.0, 37.25, 120.0, 239.0, 300.0,
        -310.0,
    ];
    const SCALES: [f32; 9] = [0.5, 0.75, 0.999, 1.0, 1.001, 1.25, 1.414, 2.0, 3.0];
    // No move is a half device pixel at 1.25 or 1.5 (152 x 1.25 = 190, 60.3 x 1.5 = 90.45).
    const MOVES: [f32; 8] = [-152.0, -60.3, -12.0, 0.0, 0.3, 12.0, 60.3, 152.0];
    const FADES: [f32; 3] = [20.0, 50.0, 80.0];
    const TURNS: [f32; 5] = [0.0, 15.0, 30.0, 90.0, -45.0];
    const NUDGES: [f32; 5] = [-40.0, -7.5, 0.0, 7.5, 40.0];
    let child = s.child;
    match rng.next() % if badge { 7 } else { 6 } {
        0 => {
            let (x, y) = (rng.pick(&PANS), rng.pick(&PANS) * 0.5);
            s.pan(x, y);
            format!("pan to ({x}, {y})")
        }
        1 => {
            // As the editor starts a zoom: the pan goes into the transform (half the time).
            let back = rng.next() % 2 == 0;
            if back {
                s.pan(0.0, 0.0);
            }
            let (k, tx, ty) = (rng.pick(&SCALES), rng.pick(&MOVES), rng.pick(&MOVES));
            s.write(child, "picture", zoom_css(tx, ty, k));
            format!("zoom: translate({tx}, {ty}) scale({k}), pan back: {back}")
        }
        2 => {
            let percent = rng.pick(&FADES);
            s.write(child, "veil", opacity_css(percent));
            format!("veil at {percent} %")
        }
        3 => {
            let (tx, ty, a) = (rng.pick(&NUDGES), rng.pick(&NUDGES), rng.pick(&TURNS));
            s.write(child, "turned", turn_css(tx, ty, a));
            format!("turned box: translate({tx}, {ty}) rotate({a}deg)")
        }
        4 => {
            s.edit();
            "the view rendered again (an edit)".to_string()
        }
        5 => {
            // A pan, a zoom and a fade in one frame.
            let (x, y) = (rng.pick(&PANS), rng.pick(&PANS) * 0.5);
            let (k, tx) = (rng.pick(&SCALES), rng.pick(&MOVES));
            let percent = rng.pick(&FADES);
            s.pan(x, y);
            s.write(child, "picture", zoom_css(tx, 0.0, k));
            s.write(child, "veil", opacity_css(percent));
            format!("pan to ({x}, {y}) + translate({tx}, 0) scale({k}) + veil at {percent} %")
        }
        _ => {
            let (tx, ty) = (rng.pick(&NUDGES), rng.pick(&NUDGES));
            s.write(DomId::ROOT_ID, "badge", move_css(tx, ty));
            format!("badge over the view: translate({tx}, {ty})")
        }
    }
}

/// How far a channel of a moved pixel may be from a fresh render's: nothing at 1 and 2, where a
/// view's snapped offset (`round(v x s) / s`) is an exact number, two levels at 1.25 and 1.5,
/// where it is not (`n / 1.25 x 1.25` is `n` to 1e-5): a group resampled at an offset that is
/// 1e-5 px off blends with weights one step apart (A2-cpu-16). A strip on the wrong side, a
/// rect that was not repainted or a move by the wrong distance differ by whole colours.
fn resample_noise(dpi: u32) -> u8 {
    if dpi % 96 == 0 {
        0
    } else {
        2
    }
}

fn run_sequence(dpi: u32, seed: u64, steps: usize, ground: bool, badge: bool) {
    let mut s = stage(dpi, ground, badge);
    let first = s.paint();
    let (n, at) = differing(&first, &s.fresh());
    assert_eq!(n, 0, "harness: the first frame is a fresh render's, first at {at:?}");
    let mut rng = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x2545_F491_4F6C_DD1D);
    let mut history: Vec<String> = Vec::new();
    for step in 0..steps {
        let what = random_step(&mut s, &mut rng, badge);
        history.push(what.clone());
        let painted = s.paint();
        let damage: FrameDamage = s.backend.last_frame_damage.clone();
        let (n, at) = differing_by_more_than(&painted, &s.fresh(), resample_noise(dpi));
        assert_eq!(
            n,
            0,
            "dpi {dpi}, seed {seed}, ground {ground}, badge {badge}, step {step} ({what}): the \
             frame the damage path painted differs from a fresh render in {n} pixels, first at \
             {at:?}. damage = {damage:?}. Steps so far: {history:#?}"
        );
    }
}

/// The damaged render is the full render, every pixel, over random sequences of the editor's
/// kind: pans of the view (whole and fractional, negative and positive, past the ground), zooms
/// of the picture by a value (scale under and over 1, the two scales next to 1), a frame inside
/// the picture's frame turned and moved, a fade, a re-render of the view, and several of them
/// in one frame. Scales 1, 1.25, 1.5 and 2.
///
/// Extends `damage_survives_a_randomised_edit_sequence` (`dll/.../headless/mod.rs`) to views,
/// offsets, transforms and opacities. A failure prints the scale, the seed, the step and every
/// step before it.
#[test]
fn a2_cpu_the_damaged_render_is_the_full_render_over_random_pans_zooms_fades_and_edits() {
    for dpi in [96_u32, 120, 144, 192] {
        for seed in [1_u64, 2, 3] {
            run_sequence(dpi, seed, 30, true, false);
        }
    }
}

/// The same with a box painted over the view's corner (the pan's move drags it: it is repainted
/// where it is and where the move left it), and with a picture that has no ground (the page
/// shows through: the move is allowed over a flat page only).
#[test]
fn a2_cpu_the_damaged_render_is_the_full_render_with_an_overlay_and_without_a_ground() {
    for dpi in [96_u32, 144] {
        for seed in [11_u64, 12] {
            run_sequence(dpi, seed, 30, true, true);
            run_sequence(dpi, seed, 30, false, false);
        }
    }
}

/// A step of the editor's own sequence.
#[derive(Debug, Clone, Copy)]
enum EditorStep {
    /// The view's window moved: the picture this far along.
    Pan(f32, f32),
    /// The zoom starts: the view's offset back to zero and `translate scale` on the picture, in
    /// one frame.
    ZoomStart(f32, f32, f32),
    /// `translate(tx, ty) scale(k)` on the picture.
    Zoom(f32, f32, f32),
    /// `translate(tx, ty)` alone on the picture: the render's scale again.
    Move(f32, f32),
}

/// The editor's own sequence, by hand: pan inside the margin, start a zoom (the pan goes into
/// the transform and the view's offset back to zero, in one frame), zoom on, zoom out past 1,
/// back to the render's scale with a move left (a translation: no group), the identity, pan
/// again.
#[test]
fn a2_cpu_the_editors_pan_then_zoom_then_pan_is_a_fresh_renders_frame_at_every_step() {
    let steps = [
        EditorStep::Pan(36.0, -20.0),
        EditorStep::Pan(60.0, -44.0),
        EditorStep::ZoomStart(29.6, -64.0, 1.19),
        EditorStep::Zoom(-12.8, -70.5, 1.414),
        EditorStep::Zoom(90.0, 20.0, 0.84),
        EditorStep::Move(60.0, -44.0),
        EditorStep::Move(0.0, 0.0),
        EditorStep::Pan(-200.0, 100.0),
    ];
    for dpi in [96_u32, 120, 192] {
        let mut s = stage(dpi, true, false);
        let _ = s.paint();
        let child = s.child;
        for step in steps {
            match step {
                EditorStep::Pan(x, y) => s.pan(x, y),
                EditorStep::ZoomStart(tx, ty, k) => {
                    s.pan(0.0, 0.0);
                    s.write(child, "picture", zoom_css(tx, ty, k));
                }
                EditorStep::Zoom(tx, ty, k) => s.write(child, "picture", zoom_css(tx, ty, k)),
                EditorStep::Move(tx, ty) => s.write(child, "picture", move_css(tx, ty)),
            }
            let painted = s.paint();
            let damage = s.backend.last_frame_damage.clone();
            let (n, at) = differing_by_more_than(&painted, &s.fresh(), resample_noise(dpi));
            assert_eq!(
                n, 0,
                "dpi {dpi}, {step:?}: {n} pixels differ from a fresh render, first at {at:?}. \
                 damage = {damage:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Hand-built lists: value damage (A2-cpu-01)
// ---------------------------------------------------------------------------------------------

/// A matrix that moves by `(tx, ty)`.
fn translation(tx: f32, ty: f32) -> azul_core::transform::ComputedTransform3D {
    azul_core::transform::ComputedTransform3D {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [tx, ty, 0.0, 1.0],
        ],
    }
}

/// A2-cpu-01. A frame whose key lost its value is painted with the matrix its list was built
/// with (`initial_transform`), and that is where its damage lands - not at the identity.
///
/// Hand-computed: frame box and content (10, 20) 100 x 50, built with `translate(100, 0)`; the
/// frame before held the value `translate(50, 0)`, this one holds none. Painted before at x
/// 60..160, now at x 110..210. The damage was `[60, 160)` and `[10, 110)` (the identity): the
/// columns 160..210 of the box were never repainted. Expected: `[60, 160)` and `[110, 210)`.
///
/// mutation: `unwrap_or(*initial_transform)` -> the identity for `new_m` in `gpu_value_damage`
/// fails here.
#[test]
fn a2_cpu_01_a_changed_frame_without_a_live_value_is_damaged_where_its_initial_transform_paints_it(
) {
    use std::collections::HashMap;

    use crate::cpurender::gpu_value_damage;

    let frame = list(vec![
        DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 7 },
            initial_transform: translation(100.0, 0.0),
            bounds: wlr(10.0, 20.0, 100.0, 50.0),
        },
        rect(10.0, 20.0, 100.0, 50.0, [200, 0, 0]),
        DisplayListItem::PopReferenceFrame,
    ]);
    let mut before = HashMap::new();
    before.insert(7_usize, translation(50.0, 0.0));
    let now = HashMap::new();
    let none = HashMap::new();
    let damage = gpu_value_damage(&frame, &before, &none, &now, &none, &ScrollOffsetMap::new());
    assert!(!damage.needs_full, "two translations: rects, no full repaint");
    let covers = |x0: f32, x1: f32| {
        damage.rects.iter().any(|r| {
            r.origin.x <= x0 + 0.01
                && r.origin.x + r.size.width >= x1 - 0.01
                && r.origin.y <= 20.01
                && r.origin.y + r.size.height >= 69.99
        })
    };
    assert!(
        covers(60.0, 160.0),
        "where the frame before painted the box (its value, translate(50, 0)): {:?}",
        damage.rects
    );
    assert!(
        covers(110.0, 210.0),
        "where this frame paints it (no value: the list's own translate(100, 0)): {:?}",
        damage.rects
    );
}

// ---------------------------------------------------------------------------------------------
// Hand-built lists: the item diff (A2-cpu-05)
// ---------------------------------------------------------------------------------------------

/// A2-cpu-05. Two lists that differ only in a text shadow (the halo's colour) are not "nothing
/// to repaint": the push has no bounds and the runs under it are equal, so the paired diff
/// answered `Some(vec![])` - an idle frame for the window's list, and no damage for the halo in a
/// view whose diff found anything else. Now the diff gives up (`None`: the caller repaints the
/// window, or the view's box).
///
/// mutation: removing the `PushTextShadow` arm of `compute_display_list_damage_impl` fails here
/// (the answer is `Some([])` again).
#[test]
fn a2_cpu_05_a_changed_text_shadow_is_not_an_idle_frame() {
    use azul_css::props::style::box_shadow::StyleBoxShadow;

    use crate::cpurender::compute_display_list_damage;

    let halo = |colour: [u8; 3]| {
        list(vec![
            rect(0.0, 0.0, 200.0, 100.0, [255, 255, 255]),
            DisplayListItem::PushTextShadow {
                shadow: StyleBoxShadow {
                    color: ColorU {
                        r: colour[0],
                        g: colour[1],
                        b: colour[2],
                        a: 255,
                    },
                    ..StyleBoxShadow::default()
                },
            },
            rect(20.0, 20.0, 60.0, 14.0, [0, 0, 0]),
            DisplayListItem::PopTextShadow,
        ])
    };
    let none = ScrollOffsetMap::new();
    let white = halo([255, 255, 255]);
    assert_eq!(
        compute_display_list_damage(&white, &halo([255, 255, 255]), &none, &none),
        Some(Vec::new()),
        "harness: the same halo is no damage"
    );
    let changed = compute_display_list_damage(&white, &halo([255, 230, 0]), &none, &none);
    assert!(
        changed.is_none(),
        "a halo that changed colour repaints its runs: the diff cannot say where and gives up, \
         it does not answer 'nothing': {changed:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Hand-built lists: a group under a clip between pixels (A2-cpu-06)
// ---------------------------------------------------------------------------------------------

/// The colour of device pixel `(x, y)`.
fn px(p: &AzulPixmap, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * p.width() + x) * 4) as usize;
    let d = p.data();
    [d[i], d[i + 1], d[i + 2]]
}

/// A2-cpu-06. An isolated group lands on the same pixels of a clip as a direct paint under
/// that clip.
///
/// Hand-computed at a scale of 1.25: a view box at logical (50, 40) 300 x 200 is the device
/// clip x 62.5 .. 437.5, y 50 .. 300. A fill painted under it writes the columns 62 .. 436 (`as
/// i32` on both edges: `agg_fill_path_clipped`, `render_rect`). The view's content inside a
/// frame scaled by 1.5 is an isolated group; its composite was clipped to `round(62.5) .. round(
/// 437.5)` = 63 .. 437: column 62 showed the page for as long as the frame was scaled, and
/// column 437 showed the content. Expected, scaled or not: column 62 the content, column 437 the
/// page.
///
/// mutation: `c.pixel_box()` -> the four `round()`s in `GroupGeometry::of` fails here.
#[test]
fn a2_cpu_06_a_scaled_group_fills_the_same_columns_of_a_clip_between_pixels_as_a_direct_paint() {
    const GROUND: [u8; 3] = [47, 49, 54];
    const PAGE: [u8; 3] = [232, 232, 232];
    let dpi = 1.25_f32;
    let parent = list(vec![
        rect(0.0, 0.0, LIST_W, LIST_H, PAGE),
        DisplayListItem::VirtualView {
            child_dom_id: CHILD,
            bounds: wlr(50.0, 40.0, 300.0, 200.0),
            clip_rect: wlr(50.0, 40.0, 300.0, 200.0),
            content_offset: LogicalPosition::zero(),
        },
    ]);
    // The view's content: a ground far larger than the view, in a frame scaled by `k` about
    // the view's corner.
    let framed = |k: f32| {
        Arc::new(list(vec![
            DisplayListItem::PushReferenceFrame {
                transform_key: azul_core::resources::TransformKey { id: 5 },
                initial_transform: azul_core::transform::ComputedTransform3D {
                    m: [
                        [k, 0.0, 0.0, 0.0],
                        [0.0, k, 0.0, 0.0],
                        [0.0, 0.0, 1.0, 0.0],
                        [0.0, 0.0, 0.0, 1.0],
                    ],
                },
                bounds: wlr(0.0, 0.0, 300.0, 200.0),
            },
            rect(-256.0, -256.0, 812.0, 712.0, GROUND),
            DisplayListItem::PopReferenceFrame,
        ]))
    };
    for (what, k) in [("painted in place (scale 1)", 1.0_f32), ("a group (scale 1.5)", 1.5)] {
        let frame = fresh_list_frame(&parent, &framed(k), dpi);
        assert_eq!(px(&frame, 250, 100), GROUND, "harness, {what}: inside the view");
        assert_eq!(px(&frame, 20, 100), PAGE, "harness, {what}: beside the view");
        assert_eq!(
            px(&frame, 62, 100),
            GROUND,
            "{what}: column 62 is the first column of the clip x 62.5 .. 437.5"
        );
        assert_eq!(
            px(&frame, 437, 100),
            PAGE,
            "{what}: column 437 is past the clip's last column, 436"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Hand-built lists: the layered full repaint against the damage repaint (A2-cpu-14)
// ---------------------------------------------------------------------------------------------

/// A2-cpu-14. A scaled frame whose box starts between two device pixels is composited at the
/// same place by a full repaint (the layered compositor) and by a damage repaint (the flat
/// walk).
///
/// Hand-computed at dpi 1: a frame box (20.5, 30.5) 40 x 30, scaled by 2 about its corner. Both
/// paths paint the box into a surface whose pixel (0, 0) is device pixel (20, 30). The flat walk
/// maps surface x `g` to `20.5 + 2 (g - 0.5) = 2 g + 19.5`. The layer was placed at
/// `round(20.5) = 21`: `21 + 2 (g - 0.5) = 2 g + 20`, half a pixel to the right (and down): the
/// frame's edges were resampled with other weights (device pixel 21: 50 % red in the damage
/// repaint, 25 % in the full one). Expected: the same pixels.
///
/// mutation: `f64::from(logical) * dpi` -> `layout_offset_device_px(logical, dpi)` for a layer
/// with a `transform_origin` in `composite_layer_recursive` fails here.
#[test]
fn a2_cpu_14_a_scaled_frame_between_pixels_is_composited_where_the_damage_repaint_puts_it() {
    use std::collections::HashMap;

    use crate::cpurender::CompositorState;

    let (w, h) = (200_u32, 140_u32);
    let page = list(vec![
        rect(0.0, 0.0, 200.0, 140.0, [255, 255, 255]),
        DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 9 },
            initial_transform: azul_core::transform::ComputedTransform3D {
                m: [
                    [2.0, 0.0, 0.0, 0.0],
                    [0.0, 2.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
            },
            bounds: wlr(20.5, 30.5, 40.0, 30.0),
        },
        rect(20.5, 30.5, 40.0, 30.0, [200, 0, 0]),
        DisplayListItem::PopReferenceFrame,
    ]);
    let resources = RendererResources::default();
    let fonts = font_manager();
    let state = CpuRenderState::new(ScrollOffsetMap::new());

    // The full repaint: the layered compositor. Nothing is painted over the frame, so it is a
    // layer of its own.
    let mut compositor = CompositorState::new(w, h);
    compositor.allocate_layers_from_display_list(&page, 1.0, &HashMap::new(), &HashMap::new());
    assert_eq!(compositor.layers.len(), 2, "harness: the scaled frame is a layer of its own");
    let mut glyphs = GlyphCache::new();
    compositor
        .render_layers(&page, 1.0, &resources, &fonts, &mut glyphs, &state)
        .expect("the layers render");
    let mut full = AzulPixmap::new(w, h).expect("a pixmap");
    full.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut full, 1.0);

    // The damage repaint: the flat walk, the whole window as one rect.
    let mut flat = AzulPixmap::new(w, h).expect("a pixmap");
    flat.fill(255, 255, 255, 255);
    let mut glyphs = GlyphCache::new();
    render_display_list_damaged(
        &page,
        &mut flat,
        1.0,
        &resources,
        &fonts,
        &mut glyphs,
        &state,
        &[lr(0.0, 0.0, 200.0, 140.0)],
    )
    .expect("the damage repaints");

    assert_eq!(px(&flat, 60, 60), [200, 0, 0], "harness: the scaled box is painted");
    let (n, first) = differing(&full, &flat);
    assert_eq!(
        n, 0,
        "the full repaint and the damage repaint of one frame differ in {n} pixels, first at \
         {first:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// A window: a border thinner than a device pixel (A2-cpu-17)
// ---------------------------------------------------------------------------------------------

/// A2-cpu-17. A solid border thinner than one device pixel is one device pixel, inside its box.
///
/// Hand-computed at dpi 1: a box at (20, 20) with `border: 0.5px solid red`. Device width
/// `0.5 as i32 = 0`: the top strip was the bar of rows `20 ..= 19` - nothing, or (normalised) the
/// rows 19 and 20, one above the box. Expected: row 20 red from column 20 on, row 19 untouched;
/// column 20 red, column 19 untouched.
///
/// mutation: `(sw as i32).max(1)` -> `sw as i32` in `render_border` fails here.
#[test]
fn a2_cpu_17_a_border_thinner_than_a_device_pixel_is_one_pixel_inside_its_box() {
    let mut dom = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(abs_box(0.0, 0.0, 200.0, 100.0, "background: #ffffff;"))
        .with_child(abs_box(20.0, 20.0, 100.0, 40.0, "border: 0.5px solid #ff0000;"));
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    lw.layout_new_generation(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let mut backend = CpuBackend::new();
    let _ = backend.render_frame(&lw, &RendererResources::default(), 200.0, 100.0, 1.0);
    let frame = backend.last_frame.as_ref().expect("a frame");
    let red = |x: u32, y: u32| {
        let [r, g, b] = px(frame, x, y);
        r > 200 && g < 60 && b < 60
    };
    assert!(red(60, 20), "the top edge, row 20: {:?}", px(frame, 60, 20));
    assert!(!red(60, 19), "nothing above the box, row 19: {:?}", px(frame, 60, 19));
    assert!(red(20, 40), "the left edge, column 20: {:?}", px(frame, 20, 40));
    assert!(!red(19, 40), "nothing left of the box, column 19: {:?}", px(frame, 19, 40));
    assert!(!red(60, 30), "nothing inside the box: {:?}", px(frame, 60, 30));
}
