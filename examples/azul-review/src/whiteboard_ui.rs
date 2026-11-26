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
    callbacks::{RenderImageCallbackInfo, TextAreaOnFocusLostCallbackType, TextAreaOnTextInputCallbackType},
    dom::{AccessibilityInfo, AccessibilityRole, AttributeType, DomNodeId, NodeId, RenderImageCallback, VirtualKeyCode},
    image::{Brush, ImageRef, RawImage, RawImageFormat},
    prelude::*,
    str::String as AzString,
    vec::U8VecRef,
    widgets::{OnTextInputReturn, TextArea, TextAreaState, TextInputValid},
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    whiteboard::{Arrow, Cutout, ImageSlot, ItemId, Notice},
    whiteboard_app::{Plan, Whiteboard},
    whiteboard_boards_ui::{self as boards_ui, Switcher},
    whiteboard_drag::{ArmedRegions, GRIP_PX},
    whiteboard_geom::{arrowhead, Curve, Pt, Size},
    whiteboard_input::{Mods, Out, PointerKind},
    whiteboard_jump::JumpTarget,
};

/// The id of the node that takes the pointer (and the window's text of it for a script).
pub const SURFACE_ID: &str = "__azreview_whiteboard-surface";

/// How long the panel's frames took, kept for a line of numbers (`AZREVIEW_PROFILE`-style): the last
/// [`Perf::KEEP`] of the surface's draws and of the panel's layouts, in microseconds.
#[derive(Debug, Clone, Default)]
pub struct Perf {
    surface_us: Vec<u32>,
    layout_us: Vec<u32>,
}

impl Perf {
    pub const KEEP: usize = 240;

    fn push(v: &mut Vec<u32>, us: u32) {
        if v.len() >= Self::KEEP {
            v.remove(0);
        }
        v.push(us);
    }

    pub fn surface(&mut self, us: u32) {
        Self::push(&mut self.surface_us, us);
    }

    pub fn layout(&mut self, us: u32) {
        Self::push(&mut self.layout_us, us);
    }

    /// `surface n=.. mean=.. max=.. us layout n=.. mean=.. max=.. us` over what was kept since the last call
    /// (and forgets it); `None` when nothing was drawn.
    pub fn take_summary(&mut self) -> Option<String> {
        let stats = |v: &[u32]| -> String {
            let n = v.len().max(1);
            let mean = v.iter().map(|x| u64::from(*x)).sum::<u64>() / n as u64;
            format!("n={} mean={mean} max={}", v.len(), v.iter().copied().max().unwrap_or(0))
        };
        if self.surface_us.is_empty() && self.layout_us.is_empty() {
            return None;
        }
        let line = format!("surface {} us, layout {} us", stats(&self.surface_us), stats(&self.layout_us));
        self.surface_us.clear();
        self.layout_us.clear();
        Some(line)
    }
}

/// The panel and what it keeps between frames.
pub struct Panel {
    pub wb: Whiteboard,
    started: Instant,
    /// The pictures as nodes use them, by cutout: the picture they are of (its address) and the image.
    images: BTreeMap<ItemId, (usize, ImageRef)>,
    /// The size of the surface in logical px, and the window's pixel scale, as the last frame had them.
    pub viewport: Size,
    pub dpr: f32,
    /// A region is being carried over the bar: the panel shows a frame inside its edge. It is a node, so the window
    /// refreshes its DOM when this changes (`Motion::Dragging { over_bar }` says it, twice in a drag).
    pub drag_over: bool,
    /// What the person was last told (a refused cutout, a repaired file).
    pub hint: Option<String>,
    /// The item whose label is being typed.
    pub editing: Option<ItemId>,
    /// The header: whether the list of boards is open, which board is being renamed, what waits for a save.
    pub switcher: Switcher,
    /// How long the frames took (asked for by `perf_on`).
    pub perf: Perf,
    pub perf_on: bool,
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
            editing: None,
            switcher: Switcher::default(),
            perf: Perf::default(),
            perf_on: false,
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

    /// A jump to a cutout's lines was made: when the file is not as it was when they were circled, the person is told
    /// (the cutout keeps its picture); when it is, a word said before is taken back.
    pub fn tell_jump(&mut self, target: &JumpTarget) {
        self.hint = jump_words(target);
    }
}

/// What a jump says about the file, in the person's language (`None` when the file is as it was).
#[must_use]
pub fn jump_words(target: &JumpTarget) -> Option<String> {
    let key = target.freshness.key()?;
    Some(t_args(
        key,
        &[("first", Arg::from(target.first_line as usize)), ("last", Arg::from(target.last_line as usize))],
    ))
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
    /// The list of boards changed or another board is to be opened: write the list, write a copy, read a board
    /// ([`Plan`]); a board that was read is told to the panel with `Whiteboard::loaded`, and then to
    /// `Panel::after_wait`.
    pub on_plan: fn(&mut RefAny, Plan, &mut CallbackInfo),
}

impl Host {
    pub(crate) fn run<R>(&self, mut f: impl FnMut(&mut Panel) -> R) -> Option<R> {
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
    // The label is shown under the cutout; while it is typed, its text area ([`editor_dom`]) is, over the surface. The
    // board's ground is a fixed light colour, so the ink on it is fixed too: the system's text colour is white in dark mode.
    if panel.editing != Some(c.id) && !c.label.is_empty() {
        group.add_child(
            Dom::create_span_with_text(c.label.as_str()).with_css(
                format!(
                    "position: absolute; left: {}; top: {}; width: {}; font-size: 11px; color: #1f1f1f; text-align: center;",
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

/// What a label's text area tells back: which cutout, and the panel it is on.
struct LabelTag {
    host: Host,
    id: ItemId,
}

/// The text area the label of item `id` (a cutout's, under it; an arrow's, on its line) is typed in. `name` is
/// what a screen reader says of it. It takes the keyboard when it mounts: the person double-clicked to type.
fn label_editor(host: &Host, id: ItemId, text: &str, name: String, at: Pt, w: f32) -> Dom {
    let tag = RefAny::new(LabelTag { host: host.clone(), id });
    TextArea::create()
        .with_text(text)
        .with_placeholder(label("azreview-whiteboard-label-placeholder"))
        .with_accessibility_name(name)
        .with_on_text_input(tag.clone(), on_label_text as TextAreaOnTextInputCallbackType)
        .with_on_focus_lost(tag, on_label_blur as TextAreaOnFocusLostCallbackType)
        .dom()
        .with_id(format!("__azreview_whiteboard-label-{id}").as_str())
        .with_attribute(AttributeType::Autofocus)
        .with_css(format!("position: absolute; left: {}; top: {}; width: {}; min-height: 24px;", css_px(at.x), css_px(at.y), css_px(w)).as_str())
}

/// The width of the box an arrow's label is centred in, px: the label is as wide as its text, up to this and then
/// some (labels are short).
const ARROW_LABEL_W: f32 = 160.0;

/// The text area the label of item `id` is being typed in, to be put over the surface (which takes every click on
/// the panel: a text area under it could not be clicked into): under a cutout, over the line of an arrow. `None` for
/// an item that is not on the board.
fn editor_dom(panel: &Panel, host: &Host, id: ItemId) -> Option<Dom> {
    let board = &panel.wb.board;
    let view = board.view();
    if let Some(c) = board.cutout(id) {
        let o = view.to_screen(c.rect.origin());
        let (w, h) = (c.rect.w * view.zoom, c.rect.h * view.zoom);
        let name = t_args(
            "azreview-whiteboard-label-name",
            &[
                ("file", Arg::from(c.source.file.clone())),
                ("first", Arg::from(c.source.first_line as usize)),
                ("last", Arg::from(c.source.last_line as usize)),
            ],
        );
        return Some(label_editor(host, id, c.label.as_str(), name, Pt::new(o.x, o.y + h + 3.0), w));
    }
    let a = board.arrow(id)?;
    let mid = view.to_screen(board.place_arrow(id)?.curve.at(0.5));
    let name = t("azreview-whiteboard-arrow-label-name");
    Some(label_editor(host, id, a.label.as_str(), name, Pt::new(mid.x - ARROW_LABEL_W * 0.5, mid.y - 36.0), ARROW_LABEL_W))
}

/// The label of an arrow, over its line at the middle: `None` for an arrow with no label, or one whose label is being
/// typed ([`editor_dom`]).
fn arrow_label_dom(panel: &Panel, a: &Arrow) -> Option<Dom> {
    if a.label.is_empty() || panel.editing == Some(a.id) {
        return None;
    }
    let board = &panel.wb.board;
    let mid = board.view().to_screen(board.place_arrow(a.id)?.curve.at(0.5));
    // The text sits in a pill that is as wide as the text, centred over the line's middle (the grip that bends a selected
    // arrow is on the middle: the pill clears it): a row that centres its one item.
    Some(
        Dom::create_div()
            .with_css(
                format!(
                    "position: absolute; left: {}; top: {}; width: {}; display: flex; flex-direction: row; justify-content: center;",
                    css_px(mid.x - ARROW_LABEL_W * 0.5),
                    css_px(mid.y - 24.0),
                    css_px(ARROW_LABEL_W)
                )
                .as_str(),
            )
            .with_child(
                Dom::create_span_with_text(a.label.as_str()).with_css(
                    "padding: 1px 7px; background: #f4f1ea; border: 1px solid #b9b4ab; border-radius: 8px; font-size: 11px; \
                     color: #1f1f1f; white-space: pre;",
                ),
            ),
    )
}

/// A character typed in a label: the label is taken when the area is left.
extern "C" fn on_label_text(_data: RefAny, _info: CallbackInfo, _state: TextAreaState) -> OnTextInputReturn {
    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
}

/// The label's text area was left: its text is the label (a step of the board), the typing is over.
extern "C" fn on_label_blur(mut data: RefAny, mut info: CallbackInfo, state: TextAreaState) -> Update {
    let Some((host, id)) = data.downcast_ref::<LabelTag>().map(|t| (t.host.clone(), t.id)) else {
        return Update::DoNothing;
    };
    let text = state.get_text().as_str().to_string();
    let changed = host.run(|p| {
        p.editing = None;
        let ok = p.wb.board.set_label(id, &text).is_ok();
        p.collect_notices();
        ok && p.wb.board.needs_save()
    });
    if changed == Some(true) {
        (host.on_changed)(&mut host.app.clone(), &mut info);
    }
    Update::RefreshDom
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
    let began = Instant::now();
    let selected = panel.wb.input.selection();
    let cutouts: Vec<Cutout> = panel.wb.board.cutouts().cloned().collect();
    // The board's own area, under the header: its origin is the surface's and the cutouts' (no border: the bar's rect,
    // where a drop is measured from, is the panel's own, and the header [`HEADER_PX`] is the area's offset in it).
    let mut area = Dom::create_div()
        .with_css("position: relative; flex-grow: 1; min-height: 0px; min-width: 0px; overflow: hidden; background: #f4f1ea;");
    for c in &cutouts {
        area.add_child(cutout_dom(panel, c, selected == Some(c.id)));
    }
    let arrows: Vec<Arrow> = panel.wb.board.arrows().cloned().collect();
    for a in &arrows {
        if let Some(on_its_line) = arrow_label_dom(panel, a) {
            area.add_child(on_its_line);
        }
    }
    if panel.wb.is_loading() {
        area.add_child(boards_ui::loading_dom());
    } else if cutouts.is_empty() {
        area.add_child(
            Dom::create_p_with_text(t("azreview-whiteboard-empty").as_str())
                .with_css("position: absolute; left: 16px; right: 16px; top: 24px; margin: 0px; color: #6b665e; font-size: 12px;"),
        );
    }
    if let Some(said) = panel.hint.as_deref() {
        // What the board told (a refusal, a repaired file): under the surface, over the cutouts, until the next press.
        area.add_child(Dom::create_p_with_text(said).with_css(
            "position: absolute; left: 8px; right: 8px; bottom: 8px; margin: 0px; padding: 6px 8px; background: #fff3cd; \
             border: 1px solid #e0c36a; border-radius: 4px; font-size: 11px; color: #5a4a10;",
        ));
    }
    if panel.drag_over {
        // A region is carried over the bar: a frame inside its edge says it will be taken. Under the surface, which
        // keeps the pointer.
        area.add_child(Dom::create_div().with_css(
            "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; border: 2px solid #205cd6;",
        ));
    }
    area.add_child(surface(host));
    // The label that is being typed: its text area is over the surface, so that a click lands in it.
    if let Some(typing) = panel.editing.and_then(|id| editor_dom(panel, host, id)) {
        area.add_child(typing);
    }
    // The panel: the header, the board under it; over both the list of boards when it is open and the bar that tells a
    // board was deleted.
    let mut root = Dom::create_div()
        .with_css(
            "position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px; \
             overflow: hidden; background: #f4f1ea;",
        )
        .with_child(boards_ui::header_dom(panel, host))
        .with_child(area);
    if panel.switcher.open {
        root.add_child(boards_ui::popover_dom(panel, host));
    }
    if let Some(toast) = panel.switcher.toast.as_ref() {
        root.add_child(boards_ui::toast_dom(toast, host));
    }
    if panel.perf_on {
        panel.perf.layout(u32::try_from(began.elapsed().as_micros()).unwrap_or(u32::MAX));
    }
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
    .with_css("position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;")
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), RefAny::new(host.clone()), on_down)
    // Movement is `MouseMove` (W3C `mouseover` fires once, on entry).
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), RefAny::new(host.clone()), on_move)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), RefAny::new(host.clone()), on_up)
    .with_callback(EventFilter::Hover(HoverEventFilter::DoubleClick), RefAny::new(host.clone()), on_double)
    .with_callback(EventFilter::Hover(HoverEventFilter::Scroll), RefAny::new(host.clone()), on_wheel)
}

// ---- drawing what is not a picture

/// How far past its curve an arrow paints, logical px: its head (11 px) and the line's width.
const ARROW_REACH: f32 = 16.0;
const ARROW: [u8; 4] = [59, 66, 82, 255];
const ACCENT: [u8; 4] = [32, 92, 214, 255];
const PEN: [u8; 4] = [214, 45, 32, 255];

fn brush(rgba: [u8; 4], radius: f32) -> Brush {
    let mut b = Brush::create(ColorU { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }, radius);
    b.hardness = 0.85;
    b.flow = 1.0;
    // The engine stamps a dab every `radius * spacing` px along a piece, each dab a disc of its radius. A quarter of
    // the nib's width between dabs (0.5 of the radius) is continuous to the eye; the tenth it was (0.2) stamped two and
    // a half times as often, and on a full board the stamping is what a frame of the surface costs (measured with
    // `sample` on the demo: 138 of the 172 samples of the surface callback were its strokes, 6 the placing of the arrows).
    b.spacing = 0.5;
    b
}

/// What is drawn on the surface.
struct Painter<'a> {
    image: &'a mut RawImage,
    /// Device pixels per logical px.
    k: f32,
    /// What the image shows, in logical px: a piece of line that lies wholly off it (with the width of its nib) paints
    /// nothing, so it is not stamped - the brush walks a piece along its whole length, and an arrow that runs far off
    /// the surface would be stamped along all of it for the sake of the part in view.
    shown: crate::whiteboard_geom::Rect,
}

impl<'a> Painter<'a> {
    fn new(image: &'a mut RawImage, k: f32) -> Self {
        let per = k.max(0.01);
        let shown = crate::whiteboard_geom::Rect::new(0.0, 0.0, image.width as f32 / per, image.height as f32 / per);
        Self { image, k, shown }
    }
}

impl Painter<'_> {
    fn line(&mut self, a: Pt, b: Pt, radius: f32, rgba: [u8; 4]) {
        // (The box of a piece along a row or a column has no height or width: the nib's reach gives it some.)
        let reach = crate::whiteboard_geom::Rect::from_points(&[a, b]).map(|r| r.inflate(radius + 1.0));
        if !reach.is_some_and(|r| r.intersects(&self.shown)) {
            return;
        }
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

    /// An arrow along `curve` (in screen px): its line and an open head at its end. The line is drawn in pieces
    /// of about ten px (at least two, so a bend shows; at most 32): each piece is a stroke of the engine's brush.
    fn arrow(&mut self, curve: &Curve, radius: f32, rgba: [u8; 4]) {
        let way = (curve.a.dist(curve.b) + curve.a.dist(curve.c) + curve.c.dist(curve.b)) * 0.5;
        let pieces = ((way / 10.0).ceil() as usize).clamp(2, 32);
        self.polyline(&curve.flatten(pieces), radius, rgba);
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
    let mut paint = Painter::new(image, k);
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

/// Paints the outline of a region `polygon` (page px) carried by `offset`, `k` device pixels to a px: what the
/// pointer holds, while a lifted region is on its way to the bar (`Ghost::offset`). The window draws it on the
/// layer under the pointer; the armed outline stays where the region lies.
pub fn paint_ghost(image: &mut RawImage, k: f32, polygon: &[Pt], offset: Pt) {
    let mut paint = Painter::new(image, k);
    let carried: Vec<Pt> = polygon.iter().chain(polygon.first()).map(|p| Pt::new(p.x + offset.x, p.y + offset.y)).collect();
    paint.polyline(&carried, 2.0, [32, 92, 214, 235]);
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
    let mut paint = Painter::new(&mut image, k);
    // What the surface shows, in logical px: an arrow wholly off it paints nothing (its control points hold the
    // curve), so a board of hundreds only pays for the arrows in view.
    let on_surface = crate::whiteboard_geom::Rect::new(0.0, 0.0, w as f32 / k.max(0.01), h as f32 / k.max(0.01));
    for (id, placed) in board.placed_arrows() {
        if preview.is_some_and(|p| p.replaces == Some(id)) {
            continue;
        }
        let selected = input.selection() == Some(id);
        let curve = screen_curve(&placed.curve);
        // (A straight arrow along a row or a column has a box of no height or width: the reach gives it some.)
        if !crate::whiteboard_geom::Rect::from_points(&[curve.a, curve.c, curve.b])
            .is_some_and(|r| r.inflate(ARROW_REACH).intersects(&on_surface))
        {
            continue;
        }
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
        let began = Instant::now();
        p.viewport = Size::new(logical.width, logical.height);
        p.dpr = k;
        let image = ImageRef::create_rawimage(paint_surface(p, w, h, k)).into_option();
        if p.perf_on {
            p.perf.surface(u32::try_from(began.elapsed().as_micros()).unwrap_or(u32::MAX));
        }
        image
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

/// What a gesture asked of the window. `keep`: the gesture is over, a change to the board is kept now (not at
/// every move of a drag).
fn answer(host: &Host, mut app: RefAny, out: Out, keep: bool, info: &mut CallbackInfo) -> Update {
    let changed = host.run(|p| {
        p.collect_notices();
        p.wb.board.needs_save()
    });
    match out {
        Out::Nothing => Update::DoNothing,
        Out::Redraw => {
            redraw(info);
            if keep && changed == Some(true) {
                (host.on_changed)(&mut app, info);
            }
            Update::DoNothing
        }
        Out::Jump(id) => (host.on_jump)(&mut app, id, info),
        Out::EditLabel(id) => {
            host.run(|p| p.editing = Some(id));
            Update::RefreshDom
        }
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
    // A board that is being read takes no pointer: what it is given would be replaced by what is read.
    if host.run(|p| p.wb.is_loading()) == Some(true) {
        return Update::DoNothing;
    }
    let (Some(at), kind, mods) = (at_of(&info), kind_of(&info), mods_of(&info)) else {
        return Update::DoNothing;
    };
    let out = host.run(|p| {
        p.hint = None;
        let Panel { wb, .. } = p;
        wb.input.press(&mut wb.board, at, kind, mods)
    });
    match out {
        // The selection is drawn on the cutout's node as well as over it: a new layout shows it.
        Some(Out::Redraw) => {
            redraw(&mut info);
            Update::RefreshDom
        }
        Some(o) => answer(&host, host.app.clone(), o, false, &mut info),
        None => Update::DoNothing,
    }
}

extern "C" fn on_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    // A board that is being read takes no pointer: what it is given would be replaced by what is read.
    if host.run(|p| p.wb.is_loading()) == Some(true) {
        return Update::DoNothing;
    }
    let (Some(at), mods) = (at_of(&info), mods_of(&info)) else {
        return Update::DoNothing;
    };
    let out = host.run(|p| {
        let Panel { wb, .. } = p;
        if wb.input.is_busy() {
            let out = wb.input.motion(&mut wb.board, at, mods);
            (out, wb.input.moves_nodes())
        } else {
            (Out::Nothing, false)
        }
    });
    match out {
        // The cutouts are nodes: they follow the pointer by a new layout, the arrows over them are redrawn with it.
        Some((Out::Redraw, true)) => {
            redraw(&mut info);
            Update::RefreshDom
        }
        Some((o, _)) => answer(&host, host.app.clone(), o, false, &mut info),
        None => Update::DoNothing,
    }
}

extern "C" fn on_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    // A board that is being read takes no pointer: what it is given would be replaced by what is read.
    if host.run(|p| p.wb.is_loading()) == Some(true) {
        return Update::DoNothing;
    }
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
        Some(o) => answer(&host, host.app.clone(), o, true, &mut info),
        None => Update::DoNothing,
    }
}

extern "C" fn on_double(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(host) = data.downcast_ref::<Host>().map(|h| (*h).clone()) else {
        return Update::DoNothing;
    };
    // A board that is being read takes no pointer: what it is given would be replaced by what is read.
    if host.run(|p| p.wb.is_loading()) == Some(true) {
        return Update::DoNothing;
    }
    let Some(at) = at_of(&info) else {
        return Update::DoNothing;
    };
    let out = host.run(|p| {
        let Panel { wb, .. } = p;
        wb.input.double_click(&mut wb.board, at)
    });
    out.map_or(Update::DoNothing, |o| answer(&host, host.app.clone(), o, false, &mut info))
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
    // A board that is being read takes no pointer: what it is given would be replaced by what is read.
    if host.run(|p| p.wb.is_loading()) == Some(true) {
        return Update::DoNothing;
    }
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
    if key == Key::Escape && panel.switcher.open {
        panel.switcher.open = false;
        panel.switcher.renaming = None;
        return true;
    }
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

/// The panel's key for a press of `code` (`None` for a key it does not answer). `primary` is the platform's
/// shortcut modifier (Cmd on macOS, Ctrl elsewhere): Delete and Backspace take the selection off, Escape lets go,
/// Mod+Z steps back, Shift+Mod+Z and Mod+Y forward.
#[must_use]
pub fn key_of(code: VirtualKeyCode, primary: bool, shift: bool) -> Option<Key> {
    match code {
        VirtualKeyCode::Delete | VirtualKeyCode::Back if !primary => Some(Key::Delete),
        VirtualKeyCode::Escape => Some(Key::Escape),
        VirtualKeyCode::Z if primary && shift => Some(Key::Redo),
        VirtualKeyCode::Z if primary => Some(Key::Undo),
        VirtualKeyCode::Y if primary => Some(Key::Redo),
        _ => None,
    }
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
