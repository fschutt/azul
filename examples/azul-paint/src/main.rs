//! AzulPaint — the P2 goal app from SUPER_PLAN_2.
//!
//! A finger / stylus paint canvas. Exercises the mobile event surface
//! that P2.1 (PenState population) and P2.2 (multi-touch TouchPointVec)
//! shipped: any stroke landed by a finger gets a fixed brush radius, any
//! stroke landed by an Apple Pencil / S-Pen gets a pressure-modulated
//! radius and per-point tilt (used for an opacity gradient).
//!
//! Strokes render as a stack of small absolutely-positioned circle
//! `<div>`s — slow with many strokes but legal in the framework's
//! current widget set (the engine has no `<canvas>` primitive yet —
//! that's a follow-up sprint). Save-as-SVG / Save-as-PNG via the
//! async file-picker handle is queued for the next tick; this tick
//! lands the canvas + strokes + the Clear button.
//!
//! Compile-only on iOS until the Xcode SDK lands. Android builds and
//! runs today via `bash scripts/build-android.sh azul-paint`.

use azul::prelude::*;

#[derive(Debug, Clone, Copy)]
struct StrokePoint {
    x: f32,
    y: f32,
    /// `0.0..=1.0`, normalized. Finger touches default to `0.5`
    /// (the TouchPoint sentinel for "no pressure available").
    pressure: f32,
    /// Barrel roll in radians (Apple Pencil Pro / Surface Pen). `0.0`
    /// when not reported. Orients the chisel nib so rolling the pen
    /// turns the brush, like a real calligraphy tip.
    barrel_roll_rad: f32,
}

#[derive(Debug, Clone)]
struct Stroke {
    points: Vec<StrokePoint>,
    /// `true` when the stroke was made by a stylus reporting eraser-tip
    /// (Apple Pencil's tip-inverted mode, Android `ToolType::Eraser`).
    is_eraser: bool,
}

struct PaintState {
    /// Committed strokes (one per pointer-down to pointer-up sequence).
    strokes: Vec<Stroke>,
    /// The stroke currently being drawn — `None` between pointer-up
    /// and the next pointer-down.
    current: Option<Stroke>,
}

impl PaintState {
    fn new() -> Self {
        Self {
            strokes: Vec::new(),
            current: None,
        }
    }

    fn begin_stroke(&mut self, p: StrokePoint, is_eraser: bool) {
        // Commit any in-flight stroke first (defensive — pointer-down
        // without a matching pointer-up shouldn't happen but the diff
        // pipeline can drop events under load).
        if let Some(active) = self.current.take() {
            if !active.points.is_empty() {
                self.strokes.push(active);
            }
        }
        self.current = Some(Stroke {
            points: vec![p],
            is_eraser,
        });
    }

    fn extend_stroke(&mut self, p: StrokePoint) {
        if let Some(s) = self.current.as_mut() {
            s.points.push(p);
        }
    }

    fn end_stroke(&mut self) {
        if let Some(active) = self.current.take() {
            if !active.points.is_empty() {
                self.strokes.push(active);
            }
        }
    }

    fn clear_all(&mut self) {
        self.strokes.clear();
        self.current = None;
    }

    fn total_points(&self) -> usize {
        self.strokes.iter().map(|s| s.points.len()).sum::<usize>()
            + self.current.as_ref().map(|s| s.points.len()).unwrap_or(0)
    }
}

// ───────── Rendering ──────────────────────────────────────────────────

const CANVAS_BG: &str = "background: #fafaf6; flex-grow: 1; position: relative; overflow: hidden;";
const HEADER_BG: &str = "background: #2b2b2b; color: white; \
    padding: 12px 20px; flex-direction: row; align-items: center; \
    justify-content: space-between; font-family: sans-serif; font-size: 16px;";
const CLEAR_BTN: &str = "background: #d04848; color: white; \
    padding: 8px 16px; border-radius: 6px; font-size: 14px; cursor: pointer;";
const ROOT: &str = "display: flex; flex-direction: column; height: 100%;";

/// Render one point as an absolutely-positioned colored circle. Radius
/// scales with pressure (so light pen-pressure → thin line, hard
/// pen-pressure → fat line). Finger touches use the 0.5 sentinel which
/// gives a uniform medium-weight stroke.
fn render_point(p: StrokePoint, is_eraser: bool) -> Dom {
    // The dab is a soft chisel nib: a rounded oval whose long axis scales
    // with pressure and whose orientation follows the pen's barrel roll.
    // With a finger / non-Pro stylus (roll = 0) it's a gentle horizontal
    // oval; rolling an Apple Pencil Pro turns it like a calligraphy tip.
    let major = (2.0 + p.pressure * 10.0).max(2.0) * 2.0;
    let minor = (major * 0.7).max(2.0);
    let left = p.x - major / 2.0;
    let top = p.y - minor / 2.0;
    let roll_deg = p.barrel_roll_rad.to_degrees();

    let color = if is_eraser {
        "rgba(250,250,246,0.95)" // eraser blends into canvas bg
    } else {
        "rgba(40,40,40,0.9)"
    };

    let style = format!(
        "position: absolute; left: {:.1}px; top: {:.1}px; \
         width: {:.1}px; height: {:.1}px; border-radius: 50%; \
         background: {}; transform: rotate({:.1}deg);",
        left, top, major, minor, color, roll_deg,
    );
    Dom::create_div().with_css(style.as_str())
}

fn render_stroke(stroke: &Stroke) -> Dom {
    let mut container = Dom::create_div();
    for p in &stroke.points {
        container = container.with_child(render_point(*p, stroke.is_eraser));
    }
    container
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    // Take a snapshot of the visible state into owned local data so we
    // can release the borrow before building the DOM (with_callback
    // needs another borrow of `data`).
    let snapshot: Option<(String, Vec<Stroke>, Option<Stroke>)> = data
        .downcast_ref::<PaintState>()
        .map(|state| {
            let active_count = state.current.as_ref().map_or(0, |_| 1);
            let header_text = format!(
                "AzulPaint — {} strokes · {} points",
                state.strokes.len() + active_count,
                state.total_points(),
            );
            (header_text, state.strokes.clone(), state.current.clone())
        });

    let Some((header_text, strokes, current)) = snapshot else {
        return Dom::create_body();
    };

    let header = Dom::create_div()
        .with_css(HEADER_BG)
        .with_child(Dom::create_text(header_text.as_str()))
        .with_child(
            Dom::create_div()
                .with_css(CLEAR_BTN)
                .with_child(Dom::create_text("Clear"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    on_clear,
                ),
        );

    let mut canvas = Dom::create_div()
        .with_css(CANVAS_BG)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            data.clone(),
            on_pointer_down,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseOver),
            data.clone(),
            on_pointer_move,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            data.clone(),
            on_pointer_up,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::TouchStart),
            data.clone(),
            on_pointer_down,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::TouchMove),
            data.clone(),
            on_pointer_move,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::TouchEnd),
            data.clone(),
            on_pointer_up,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::TouchCancel),
            data,
            on_pointer_up,
        );

    for stroke in &strokes {
        canvas = canvas.with_child(render_stroke(stroke));
    }
    if let Some(active) = current.as_ref() {
        canvas = canvas.with_child(render_stroke(active));
    }

    Dom::create_body()
        .with_css(ROOT)
        .with_child(header)
        .with_child(canvas)
}

// ───────── Callbacks ──────────────────────────────────────────────────

fn extract_point(info: &CallbackInfo) -> Option<(StrokePoint, bool)> {
    // Prefer the stylus path: it gives pressure + is_eraser.
    if let Some(pen) = info.get_pen_state().into_option() {
        if pen.in_contact {
            return Some((
                StrokePoint {
                    x: pen.position.x,
                    y: pen.position.y,
                    pressure: pen.pressure.max(0.05).min(1.0),
                    barrel_roll_rad: pen.barrel_roll_rad,
                },
                pen.is_eraser,
            ));
        }
    }
    // Fall back to cursor-relative-to-node (works for both mouse and
    // touch on every backend). No barrel roll off a stylus → 0.
    let pos = info.get_cursor_relative_to_node().into_option()?;
    Some((
        StrokePoint {
            x: pos.x,
            y: pos.y,
            pressure: 0.5,
            barrel_roll_rad: 0.0,
        },
        false,
    ))
}

extern "C" fn on_pointer_down(mut data: RefAny, info: CallbackInfo) -> Update {
    let (point, is_eraser) = match extract_point(&info) {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state.begin_stroke(point, is_eraser);
    Update::RefreshDom
}

extern "C" fn on_pointer_move(mut data: RefAny, info: CallbackInfo) -> Update {
    // Only extend the current stroke if one is in flight. Hover-without-
    // contact (Pencil hover, mouse-over without buttons) returns
    // `in_contact = false` from get_pen_state and we ignore it here.
    {
        let read = match data.downcast_ref::<PaintState>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        if read.current.is_none() {
            return Update::DoNothing;
        }
    }
    let (point, _is_eraser) = match extract_point(&info) {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state.extend_stroke(point);
    Update::RefreshDom
}

extern "C" fn on_pointer_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    let mut state_guard = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state_guard.end_stroke();
    Update::RefreshDom
}

extern "C" fn on_clear(mut data: RefAny, _info: CallbackInfo) -> Update {
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state.clear_all();
    Update::RefreshDom
}

fn main() {
    let data = RefAny::new(PaintState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
