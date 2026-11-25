//! The whiteboard's panel: the board of a workspace as the bar on the right shows it. A thin skin on
//! [`crate::whiteboard_app::Whiteboard`]: the cutouts are image nodes (their pictures kept between
//! frames), and ONE image node over them takes the pointer and draws what is not a picture - the
//! arrows, the selection, the arrow being drawn - at the window's pixel scale with the engine's brush
//! (as the ink of a page and AzPhoto's canvas are drawn).
//!
//! The app that holds the panel gives it a [`Host`]: how to reach the panel from the app's state, and
//! what to do when a cutout is double-clicked (go to its lines) or the board changed (keep it).

use std::{collections::BTreeMap, time::Instant};

use azul::{
    callbacks::RenderImageCallbackInfo,
    dom::{AccessibilityInfo, AccessibilityRole, DomNodeId, NodeId, RenderImageCallback},
    image::{Brush, ImageRef, RawImage, RawImageFormat},
    prelude::*,
    str::String as AzString,
    vec::U8VecRef,
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    whiteboard::{Cutout, ImageSlot, ItemId, Notice},
    whiteboard_app::Whiteboard,
    whiteboard_drag::{ArmedRegions, GRIP_PX},
    whiteboard_geom::{arrowhead, Curve, Pt, Size},
    whiteboard_input::{Mods, Out, PointerKind},
};

/// The id of the node that takes the pointer (and the window's text of it for a script).
pub const SURFACE_ID: &str = "__azreview_whiteboard-surface";

/// The panel and what it keeps between frames.
pub struct Panel {
    pub wb: Whiteboard,
    started: Instant,
    /// The pictures as nodes use them, by cutout: the picture they are of (its address) and the image.
    images: BTreeMap<ItemId, (usize, ImageRef)>,
    /// The size of the surface in logical px, and the window's pixel scale, as the last frame had them.
    pub viewport: Size,
    pub dpr: f32,
    /// A region is being carried over the bar.
    pub drag_over: bool,
    /// What the person was last told (a refused cutout, a repaired file).
    pub hint: Option<String>,
}

impl Panel {
    #[must_use]
    pub fn new(workspace: &str) -> Self {
        Self {
            wb: Whiteboard::new(workspace),
            started: Instant::now(),
            images: BTreeMap::new(),
            viewport: Size::new(340.0, 600.0),
            dpr: 1.0,
            drag_over: false,
            hint: None,
        }
    }

    /// Milliseconds since the panel was made: the clock the lift and the arming time themselves by.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The image node's picture for cutout `c`: kept while the cutout's picture is the same.
    pub fn image_of(&mut self, c: &Cutout) -> Option<ImageRef> {
        let ImageSlot::Ready(img) = &c.image else {
            self.images.remove(&c.id);
            return None;
        };
        let key = std::sync::Arc::as_ptr(img) as usize;
        if let Some((k, image)) = self.images.get(&c.id) {
            if *k == key {
                return Some(image.clone());
            }
        }
        let raw = RawImage::create_rgba8(img.width, img.height, img.rgba.clone(), true);
        let image = ImageRef::create_rawimage(raw).into_option()?;
        self.images.insert(c.id, (key, image.clone()));
        Some(image)
    }

    /// Tells the person what the whiteboard has to say since it was last asked.
    pub fn collect_notices(&mut self) {
        for n in self.wb.take_notices() {
            self.hint = Some(say(&n));
        }
    }
}

/// A notice in the person's language.
#[must_use]
pub fn say(n: &Notice) -> String {
    let args: Vec<(&str, Arg)> =
        n.args().into_iter().map(|(k, v)| (k, Arg::from(usize::try_from(v).unwrap_or(usize::MAX)))).collect();
    t_args(n.key(), &args)
}

/// What the panel's callbacks reach the app through.
#[derive(Clone)]
pub struct Host {
    pub app: RefAny,
    /// Runs `f` on the panel inside the app's state; whether the app has one.
    pub with: fn(&mut RefAny, &mut dyn FnMut(&mut Panel)) -> bool,
    /// A cutout was double-clicked: go to its lines (and flash them).
    pub on_jump: fn(&mut RefAny, ItemId, &mut CallbackInfo) -> Update,
    /// The board changed: keep it (a save on a Thread).
    pub on_changed: fn(&mut RefAny, &mut CallbackInfo),
}

impl Host {
    fn run<R>(&self, mut f: impl FnMut(&mut Panel) -> R) -> Option<R> {
        let mut app = self.app.clone();
        let mut out = None;
        (self.with)(&mut app, &mut |p| out = Some(f(p)));
        out
    }
}

fn css_px(v: f32) -> String {
    format!("{v:.2}px")
}

/// One cutout as a node: its picture (or the lines it was made from while there is none), its label under it.
fn cutout_dom(panel: &mut Panel, c: &Cutout, selected: bool) -> Dom {
    let view = panel.wb.board.view();
    let o = view.to_screen(c.rect.origin());
    let (w, h) = (c.rect.w * view.zoom, c.rect.h * view.zoom);
    let mut node = Dom::create_div()
        .with_css(
            format!(
                "position: absolute; left: {}; top: {}; width: {}; height: {}; overflow: hidden; background: #ffffff; \
                 box-shadow: 0px 1px 4px rgba(0,0,0,0.3); border: 1px solid {};",
                css_px(o.x),
                css_px(o.y),
                css_px(w),
                css_px(h),
                if selected { "#205cd6" } else { "#b9b4ab" },
            )
            .as_str(),
        )
        .with_id(AzString::from(format!("__azreview_whiteboard-cutout-{}", c.id)))
        .with_accessibility_info(AccessibilityInfo::named(
            AzString::from(cutout_name(c)),
            AccessibilityRole::Graphic,
        ));
    match panel.image_of(c) {
        Some(image) => {
            node.add_child(Dom::create_image(image).with_css("width: 100%; height: 100%;"));
        }
        None => {
            let why = match &c.image {
                ImageSlot::Pending => t("azreview-whiteboard-drawing"),
                _ => t("azreview-whiteboard-picture-missing"),
            };
            node.add_child(Dom::create_p_with_text(why.as_str()).with_css("margin: 0px; padding: 4px 6px; font-size: 10px; color: #6b665e;"));
            for line in c.source.lines.iter().take(12) {
                node.add_child(
                    Dom::create_span_with_text(line.as_str())
                        .with_css("display: block; padding: 0px 6px; font-family: monospace; font-size: 10px; color: #1f1f1f; white-space: pre;"),
                );
            }
        }
    }
    let mut group = Dom::create_div().with_child(node);
    if !c.label.is_empty() {
        group.add_child(
            Dom::create_span_with_text(c.label.as_str()).with_css(
                format!(
                    "position: absolute; left: {}; top: {}; width: {}; font-size: 11px; color: system:text; text-align: center;",
                    css_px(o.x),
                    css_px(o.y + h + 3.0),
                    css_px(w)
                )
                .as_str(),
            ),
        );
    }
    group
}

/// The name of a cutout for a screen reader: where it comes from, and its label.
#[must_use]
pub fn cutout_name(c: &Cutout) -> String {
    let place = t_args(
        "azreview-whiteboard-cutout-name",
        &[
            ("file", Arg::from(c.source.file.clone())),
            ("first", Arg::from(c.source.first_line as usize)),
            ("last", Arg::from(c.source.last_line as usize)),
        ],
    );
    if c.label.is_empty() {
        place
    } else {
        format!("{}: {place}", c.label)
    }
}

/// The panel: the cutouts, and over them the surface that takes the pointer.
pub fn panel_dom(panel: &mut Panel, host: &Host) -> Dom {
    let selected = panel.wb.input.selection();
    let cutouts: Vec<Cutout> = panel.wb.board.cutouts().cloned().collect();
    let mut root = Dom::create_div().with_css(
        format!(
            "position: relative; flex-grow: 1; min-height: 0px; min-width: 0px; overflow: hidden; background: #f4f1ea; \
             border: 2px solid {};",
            if panel.drag_over { "#205cd6" } else { "transparent" }
        )
        .as_str(),
    );
    for c in &cutouts {
        root.add_child(cutout_dom(panel, c, selected == Some(c.id)));
    }
    if cutouts.is_empty() {
        root.add_child(
            Dom::create_p_with_text(t("azreview-whiteboard-empty").as_str())
                .with_css("position: absolute; left: 16px; right: 16px; top: 24px; margin: 0px; color: #6b665e; font-size: 12px;"),
        );
    }
    root.add_child(surface(host));
    root
}

/// The node over the cutouts: it draws the arrows and takes the pointer.
fn surface(host: &Host) -> Dom {
    Dom::create_image(ImageRef::callback(
        RenderImageCallback::create(render_surface).to_core(),
        RefAny::new(host.clone()),
    ))
    .with_id(AzString::from_const_str(SURFACE_ID))
    .with_accessibility_info(AccessibilityInfo::named(label("azreview-whiteboard-surface"), AccessibilityRole::Graphic))
    .with_css("position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px;")
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), RefAny::new(host.clone()), on_down)
    // Movement is `MouseMove` (W3C `mouseover` fires once, on entry).
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), RefAny::new(host.clone()), on_move)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), RefAny::new(host.clone()), on_up)
    .with_callback(EventFilter::Hover(HoverEventFilter::DoubleClick), RefAny::new(host.clone()), on_double)
    .with_callback(EventFilter::Hover(HoverEventFilter::Scroll), RefAny::new(host.clone()), on_wheel)
}

// ---- drawing what is not a picture

const ARROW: [u8; 4] = [59, 66, 82, 255];
const ACCENT: [u8; 4] = [32, 92, 214, 255];
const PEN: [u8; 4] = [214, 45, 32, 255];

fn brush(rgba: [u8; 4], radius: f32) -> Brush {
    let mut b = Brush::create(ColorU { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }, radius);
    b.hardness = 0.85;
    b.flow = 1.0;
    b.spacing = 0.2;
    b
}

/// What is drawn on the surface.
struct Painter<'a> {
    image: &'a mut RawImage,
    /// Device pixels per logical px.
    k: f32,
}

impl Painter<'_> {
    fn line(&mut self, a: Pt, b: Pt, radius: f32, rgba: [u8; 4]) {
        self.image.paint_stroke(a.x * self.k, a.y * self.k, b.x * self.k, b.y * self.k, brush(rgba, radius * self.k));
    }

    fn dot(&mut self, p: Pt, radius: f32, rgba: [u8; 4]) {
        self.line(p, p, radius, rgba);
    }

    fn polyline(&mut self, points: &[Pt], radius: f32, rgba: [u8; 4]) {
        for w in points.windows(2) {
            self.line(w[0], w[1], radius, rgba);
        }
    }

    /// An arrow along `curve` (in screen px): its line and an open head at its end.
    fn arrow(&mut self, curve: &Curve, radius: f32, rgba: [u8; 4]) {
        self.polyline(&curve.flatten(32), radius, rgba);
        let [tip, a, b] = arrowhead(curve.b, curve.tangent_at(1.0), 11.0, 5.0);
        self.line(a, tip, radius, rgba);
        self.line(b, tip, radius, rgba);
    }

    fn rect(&mut self, r: crate::whiteboard_geom::Rect, radius: f32, rgba: [u8; 4]) {
        let (a, b, c, d) = (Pt::new(r.x, r.y), Pt::new(r.right(), r.y), Pt::new(r.right(), r.bottom()), Pt::new(r.x, r.bottom()));
        self.polyline(&[a, b, c, d, a], radius, rgba);
    }
}

/// Paints the armed regions of `page` into `image` (`k` device pixels to a page px): each one's outline and,
/// on the outline at the top right, its grip - the badge a press lifts the region by.
pub fn paint_armed(image: &mut RawImage, k: f32, regions: &ArmedRegions, page: usize, now_ms: u64) {
    let mut paint = Painter { image, k };
    for r in regions.all().iter().filter(|r| r.page == page && now_ms < r.expires_at_ms()) {
        let mut outline = r.polygon.clone();
        if let Some(first) = outline.first().copied() {
            outline.push(first);
        }
        paint.polyline(&outline, 1.2, [32, 92, 214, 190]);
        let c = r.grip.center();
        paint.dot(c, GRIP_PX * 0.5, ACCENT);
        // An arrow pointing right, in white: "take this along".
        let white = [255, 255, 255, 255];
        paint.line(Pt::new(c.x - 5.0, c.y), Pt::new(c.x + 5.0, c.y), 1.2, white);
        paint.line(Pt::new(c.x + 1.0, c.y - 4.0), Pt::new(c.x + 5.0, c.y), 1.2, white);
        paint.line(Pt::new(c.x + 1.0, c.y + 4.0), Pt::new(c.x + 5.0, c.y), 1.2, white);
    }
}

/// The surface's picture: the arrows, the selection, the arrow being drawn, the pen's stroke.
fn paint_surface(panel: &Panel, w: u32, h: u32, k: f32) -> RawImage {
    let mut image = RawImage::create_rgba8(w, h, vec![0_u8; w as usize * h as usize * 4], true);
    let board = &panel.wb.board;
    let view = board.view();
    let input = &panel.wb.input;
    let preview = input.preview(board);
    let screen = |p: Pt| view.to_screen(p);
    let screen_curve = |c: &Curve| Curve { a: screen(c.a), c: screen(c.c), b: screen(c.b) };
    let mut paint = Painter { image: &mut image, k };
    for a in board.arrows() {
        if preview.is_some_and(|p| p.replaces == Some(a.id)) {
            continue;
        }
        let Some(placed) = board.place_arrow(a.id) else { continue };
        let selected = input.selection() == Some(a.id);
        let curve = screen_curve(&placed.curve);
        paint.arrow(&curve, if selected { 2.0 } else { 1.5 }, if selected { ACCENT } else { ARROW });
        if selected {
            for p in [curve.a, curve.b, curve.at(0.5)] {
                paint.dot(p, 5.0, ACCENT);
            }
        }
    }
    if let Some(c) = input.selection().and_then(|id| board.cutout(id)) {
        let r = c.rect;
        let (o, size) = (screen(r.origin()), (r.w * view.zoom, r.h * view.zoom));
        paint.rect(crate::whiteboard_geom::Rect::new(o.x, o.y, size.0, size.1), 1.0, ACCENT);
        paint.dot(Pt::new(o.x + size.0, o.y + size.1), 5.0, ACCENT);
    }
    if let Some(p) = preview {
        let curve = Curve::bent(screen(p.from), screen(p.to), p.bend);
        paint.arrow(&curve, 1.5, ACCENT);
        if let Some(c) = p.target.and_then(|id| board.cutout(id)) {
            let o = screen(c.rect.origin());
            paint.rect(crate::whiteboard_geom::Rect::new(o.x, o.y, c.rect.w * view.zoom, c.rect.h * view.zoom), 1.5, ACCENT);
        }
    }
    let stroke: Vec<Pt> = input.stroke().iter().map(|p| screen(*p)).collect();
    paint.polyline(&stroke, 1.2, PEN);
    image
}

/// The surface's `RenderImageCallback`: its frame, at the pixel scale of the window.
extern "C" fn render_surface(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    let bounds = info.get_bounds();
    let (physical, logical) = (bounds.get_physical_size(), bounds.get_logical_size());
    let (w, h) = (physical.width.max(1), physical.height.max(1));
    let blank = || ImageRef::null_image(w as usize, h as usize, RawImageFormat::RGBA8, U8VecRef::from(&[][..]));
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return blank();
    };
    let k = if logical.width > 0.0 { w as f32 / logical.width } else { 1.0 };
    host.run(|p| {
        p.viewport = Size::new(logical.width, logical.height);
        p.dpr = k;
        ImageRef::create_rawimage(paint_surface(p, w, h, k)).into_option()
    })
    .flatten()
    .unwrap_or_else(blank)
}

// ---- the pointer

fn kind_of(info: &CallbackInfo) -> PointerKind {
    match info.get_pen_state().into_option() {
        Some(pen) if pen.in_contact => PointerKind::Pen,
        _ => PointerKind::Mouse,
    }
}

fn mods_of(info: &CallbackInfo) -> Mods {
    let k = info.get_key_modifiers();
    Mods { shift: k.shift, alt: k.alt, cmd: k.primary_down() }
}

fn at_of(info: &CallbackInfo) -> Option<Pt> {
    info.get_cursor_relative_to_node().into_option().map(|p| Pt::new(p.x, p.y))
}

/// Asks for a new frame of the surface under the pointer, no new DOM.
fn redraw(info: &mut CallbackInfo) {
    let node = info.get_hit_node();
    azul_appkit::ui::redraw_image(info, node);
}

/// What a gesture asked of the window.
fn answer(host: &Host, mut app: RefAny, out: Out, info: &mut CallbackInfo) -> Update {
    let changed = host.run(|p| {
        p.collect_notices();
        p.wb.board.needs_save()
    });
    match out {
        Out::Nothing => Update::DoNothing,
        Out::Redraw => {
            redraw(info);
            if changed == Some(true) {
                (host.on_changed)(&mut app, info);
            }
            Update::DoNothing
        }
        Out::Jump(id) => (host.on_jump)(&mut app, id, info),
        Out::EditLabel(_) => Update::RefreshDom,
        Out::Notice(n) => {
            host.run(|p| p.hint = Some(say(&n)));
            Update::RefreshDom
        }
    }
}

extern "C" fn on_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let buttons = info.get_current_mouse_state();
    if buttons.right_down || buttons.middle_down {
        return Update::DoNothing;
    }
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    let (Some(at), kind, mods) = (at_of(&info), kind_of(&info), mods_of(&info)) else {
        return Update::DoNothing;
    };
    let out = host.run(|p| {
        let Panel { wb, .. } = p;
        wb.input.press(&mut wb.board, at, kind, mods)
    });
    out.map_or(Update::DoNothing, |o| answer(&host, host.app.clone(), o, &mut info))
}

extern "C" fn on_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    let (Some(at), mods) = (at_of(&info), mods_of(&info)) else {
        return Update::DoNothing;
    };
    let out = host.run(|p| {
        let Panel { wb, .. } = p;
        if wb.input.is_busy() {
            wb.input.motion(&mut wb.board, at, mods)
        } else {
            Out::Nothing
        }
    });
    out.map_or(Update::DoNothing, |o| answer(&host, host.app.clone(), o, &mut info))
}

extern "C" fn on_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    let (Some(at), mods) = (at_of(&info), mods_of(&info)) else {
        return Update::DoNothing;
    };
    let out = host.run(|p| {
        let Panel { wb, .. } = p;
        wb.input.release(&mut wb.board, at, mods)
    });
    // The cutouts and the labels are nodes: what a gesture changed in them needs a new DOM as well.
    match out {
        Some(Out::Redraw) => {
            redraw(&mut info);
            let changed = host.run(|p| p.wb.board.needs_save()) == Some(true);
            if changed {
                (host.on_changed)(&mut host.app.clone(), &mut info);
            }
            Update::RefreshDom
        }
        Some(o) => answer(&host, host.app.clone(), o, &mut info),
        None => Update::DoNothing,
    }
}

extern "C" fn on_double(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    let Some(at) = at_of(&info) else {
        return Update::DoNothing;
    };
    let out = host.run(|p| {
        let Panel { wb, .. } = p;
        wb.input.double_click(&mut wb.board, at)
    });
    out.map_or(Update::DoNothing, |o| answer(&host, host.app.clone(), o, &mut info))
}

extern "C" fn on_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let node = NodeId::create(hit.node.into_raw().saturating_sub(1));
    let Some(delta) = info.get_scroll_delta(hit.dom, node).into_option() else {
        return Update::DoNothing;
    };
    if delta.x == 0.0 && delta.y == 0.0 {
        return Update::DoNothing;
    }
    info.prevent_default();
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    let (Some(at), mods) = (at_of(&info), mods_of(&info)) else {
        return Update::DoNothing;
    };
    // The wheel moves the board against the hand: the board point under the pointer follows it.
    let out = host.run(|p| {
        let Panel { wb, .. } = p;
        wb.input.wheel(&mut wb.board, at, Pt::new(-delta.x, -delta.y), mods)
    });
    // The cutouts are nodes: the view moved, so their places did.
    out.map(|_| Update::RefreshDom).unwrap_or(Update::DoNothing)
}

/// The keys the panel answers when it has the keyboard: Delete takes the selection off, Escape lets go.
/// `true` when it did something (the app then does not treat the key as its own).
pub fn on_key(panel: &mut Panel, key: Key) -> bool {
    let Panel { wb, .. } = panel;
    match key {
        Key::Delete => matches!(wb.input.delete_selection(&mut wb.board), Out::Redraw),
        Key::Escape => wb.escape(),
        Key::Undo => wb.board.undo(),
        Key::Redo => wb.board.redo(),
    }
}

/// The keys of [`on_key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Delete,
    Escape,
    Undo,
    Redo,
}

/// Lets the app say where the surface node is (for a script that clicks it).
#[must_use]
pub fn surface_node(info: &CallbackInfo) -> Option<DomNodeId> {
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, SURFACE_ID);
    (node.into_raw() != 0).then_some(DomNodeId { dom, node })
}

#[cfg(test)]
#[path = "whiteboard_ui_test.rs"]
mod whiteboard_ui_test;
