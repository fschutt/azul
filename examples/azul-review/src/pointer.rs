//! The pointer's way through a stroke, and the log of what the pointer did.
//!
//! A stroke begins with a press on a page's pen layer and ends with the release, wherever that is: the
//! layer hears a release over it, the window hears every release (`LeftMouseUp`), and a stroke whose
//! release no one heard ends at the next move that finds the pointer not pressed, at the pointer
//! leaving the window, at the window losing its focus, and at the next press. A stroke that never
//! ended would grow with every move of the hovering pointer, from the last pressed point: the ink
//! would follow the cursor, stuck to the point where it was last pressed.
//!
//! All of it is [`AppState`] state, so a sequence of events is a test without a window
//! (`pointer_test.rs`).
//!
//! The log keeps the last [`LOG_CAPACITY`] events the handlers saw (what they were, where, whether the
//! pointer was pressed, whether a stroke was live): `AZREVIEW_EVENTS=1` prints each as it comes
//! (`AZREVIEW_EVENTS=<file>` appends it to the file), the Session menu saves the log into the
//! session folder, and the debug server answers `azreview_pointer_log` with it, so a report of a
//! stroke that went wrong can carry what happened.

use std::{collections::VecDeque, io::Write, path::PathBuf, time::Instant};

use azul::{
    error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString,
};

use crate::{
    ink::Layer,
    model::{InkPoint, Stroke, VoiceClip},
    AppState, CLIP_RATE,
};

/// How many events the log keeps.
pub const LOG_CAPACITY: usize = 200;

/// A stroke of fewer points than this is a tap (it cycles the nib), not ink.
pub const CLICK_POINT_LIMIT: usize = 3;

/// How far a point must be from the stroke's last one to be a new point of it (|dx| + |dy|, px).
const MIN_STEP: f32 = 0.35;

/// What a pointer event is, as the log names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A press on a page's pen layer.
    Down,
    /// A move over a page's pen layer (pressed or not).
    Move,
    /// A release over a page's pen layer.
    Up,
    /// A release anywhere in the window.
    WindowUp,
    /// The pointer left a page's pen layer.
    Leave,
    /// The pointer left the window.
    WindowLeave,
    /// The window lost its focus.
    FocusLost,
}

impl Kind {
    /// The word the log shows.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Move => "move",
            Self::Up => "up",
            Self::WindowUp => "window-up",
            Self::Leave => "leave",
            Self::WindowLeave => "window-leave",
            Self::FocusLost => "focus-lost",
        }
    }
}

/// One pointer event as the log keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Logged {
    /// What happened.
    pub kind: Kind,
    /// The page of the pen layer it happened on (none for an event of the window).
    pub page: Option<usize>,
    /// Where, in the layer's px (0 for an event of the window).
    pub x: f32,
    pub y: f32,
    /// The pen's pressure (0 for a mouse or a trackpad).
    pub pressure: f32,
    /// A button or the pen's tip is down, as the window says.
    pub pressed: bool,
    /// A pen drives the pointer.
    pub pen: bool,
    /// A stroke was live when the event came, and how many points it had.
    pub live: Option<usize>,
}

/// Where the events are echoed as they come.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Echo {
    /// Nowhere: the log only keeps them.
    #[default]
    Off,
    /// Standard error.
    Stderr,
    /// Appended to a file.
    File(PathBuf),
}

impl Echo {
    /// What `AZREVIEW_EVENTS` says: unset or empty or `0` is off, `1` (or any word that is not a path)
    /// is standard error, a path is that file.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("" | "0" | "off") => Self::Off,
            Some(v) if v.contains('/') || v.contains('\\') => Self::File(PathBuf::from(v)),
            Some(_) => Self::Stderr,
        }
    }
}

/// The last pointer events.
#[derive(Debug)]
pub struct PointerLog {
    /// The log's clock starts here.
    started: Instant,
    ring: VecDeque<(u64, u64, Logged)>,
    /// How many events were ever logged.
    total: u64,
    echo: Echo,
}

impl Default for PointerLog {
    fn default() -> Self {
        Self::new(Echo::Off)
    }
}

impl PointerLog {
    /// An empty log that echoes to `echo`.
    #[must_use]
    pub fn new(echo: Echo) -> Self {
        Self {
            started: Instant::now(),
            ring: VecDeque::with_capacity(LOG_CAPACITY),
            total: 0,
            echo,
        }
    }

    /// Notes `event`; the oldest goes when the log is full.
    pub fn push(&mut self, event: Logged) {
        self.total += 1;
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if self.ring.len() >= LOG_CAPACITY {
            self.ring.pop_front();
        }
        let line = line_of(self.total, ms, &event);
        self.ring.push_back((self.total, ms, event));
        match &self.echo {
            Echo::Off => {}
            Echo::Stderr => eprintln!("{line}"),
            Echo::File(path) => {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                    let _written = writeln!(f, "{line}");
                }
            }
        }
    }

    /// How many events the log holds now.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// How many events were logged since the start (the log keeps the last [`LOG_CAPACITY`]).
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }

    /// The events kept, oldest first.
    #[must_use]
    pub fn events(&self) -> Vec<Logged> {
        self.ring.iter().map(|(_, _, e)| e.clone()).collect()
    }

    /// The log as text, a line per event, oldest first: what a report of a stroke that went wrong
    /// carries.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (n, ms, e) in &self.ring {
            out.push_str(&line_of(*n, *ms, e));
            out.push('\n');
        }
        out
    }

    /// The log as a JSON array of objects (the debug server's answer).
    #[must_use]
    pub fn json(&self) -> String {
        let mut out = String::from("[");
        for (i, (n, ms, e)) in self.ring.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"n\":{n},\"ms\":{ms},\"kind\":\"{}\",\"page\":{},\"x\":{:.2},\"y\":{:.2},\"pressure\":{:.2},\
                 \"pressed\":{},\"pen\":{},\"live\":{}}}",
                e.kind.name(),
                e.page.map_or_else(|| "null".to_string(), |p| p.to_string()),
                finite(e.x),
                finite(e.y),
                finite(e.pressure),
                e.pressed,
                e.pen,
                e.live.map_or_else(|| "null".to_string(), |p| p.to_string()),
            ));
        }
        out.push(']');
        out
    }
}

/// `v`, or 0 where it is not a number a JSON text can hold.
fn finite(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// One line of the log.
fn line_of(n: u64, ms: u64, e: &Logged) -> String {
    format!(
        "AZREVIEW_EVENT #{n} t={ms}ms {} page={} x={:.1} y={:.1} pressure={:.2} pressed={} pen={} live={}",
        e.kind.name(),
        e.page.map_or_else(|| "-".to_string(), |p| p.to_string()),
        finite(e.x),
        finite(e.y),
        finite(e.pressure),
        u8::from(e.pressed),
        u8::from(e.pen),
        e.live.map_or_else(|| "no".to_string(), |p| format!("{p}pts")),
    )
}

/// What a pointer lifting did to the stroke it had down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Up {
    /// A tap of a pen: it cycles the nib.
    Click,
    /// A tap that changes nothing: while a dictation listens, or by a mouse or a trackpad (a tap on a trackpad is a
    /// click: it must not change the nib under the user's hand, nor select the audio pen, whose microphone asks
    /// for a permission).
    Dab,
    /// A stroke, kept.
    Kept,
}

/// A stroke that ended: its layer and page (to be drawn anew) and what it came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ended {
    pub layer: Layer,
    pub page: usize,
    pub up: Up,
}

/// What a move did to the stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moved {
    /// No stroke was live, or the point added nothing.
    Nothing,
    /// The stroke on this layer got a point.
    Grew(Layer),
    /// The pointer was not pressed: the stroke whose release was not heard is over.
    Ended(Ended),
}

/// What a press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Began {
    /// The layer of the new stroke.
    pub layer: Layer,
    /// A stroke whose release was not heard, ended by this press.
    pub stale: Option<Ended>,
}

/// The update a move may ask the window for: none while a stroke is live (a rebuilt window
/// replaces the pen's layer under the stroke), what it was otherwise.
#[must_use]
pub fn refresh_after_move(stroke_live: bool, asked: azul::callbacks::Update) -> azul::callbacks::Update {
    if stroke_live {
        azul::callbacks::Update::DoNothing
    } else {
        asked
    }
}

impl AppState {
    /// A press at `point` of `page`: a stroke begins there. A stroke still live (its release was not
    /// heard) ends first, kept as it was. `pen`: the pointer is a pen (a mouse or a trackpad is not).
    pub fn stroke_begin(&mut self, page: usize, point: InkPoint, pen: bool) -> Began {
        let stale = self.stroke_end();
        self.live_pen = pen;
        let id = self.next_stroke_id;
        self.next_stroke_id += 1;
        let semantic = self.tool.semantic_for(self.active);
        let epoch = self.epoch;
        self.live = Some(Stroke {
            page,
            semantic,
            points: vec![point],
            id,
            epoch,
        });
        if self.tool.records_audio() && self.recording.is_none() {
            self.recording = Some(VoiceClip {
                sample_rate: CLIP_RATE,
                ..VoiceClip::default()
            });
        }
        if let Some(clip) = self.recording.as_mut() {
            clip.stroke_ids.push(id);
        }
        Began {
            layer: Layer::of(semantic),
            stale,
        }
    }

    /// The pointer is at `point`; `pressed`: a button or the pen's tip is down. While a stroke is live
    /// the point is its next one - when the pointer is pressed. A stroke live under a pointer that is
    /// not pressed lost its release: it ends here (it would grow, from the last pressed point, with every
    /// move of the hovering pointer).
    pub fn stroke_move(&mut self, point: InkPoint, pressed: bool) -> Moved {
        if self.live.is_none() {
            return Moved::Nothing;
        }
        if !pressed {
            return self.stroke_end().map_or(Moved::Nothing, Moved::Ended);
        }
        let Some(live) = self.live.as_mut() else {
            return Moved::Nothing;
        };
        let far = live
            .points
            .last()
            .is_none_or(|l| (l.x - point.x).abs() + (l.y - point.y).abs() > MIN_STEP);
        if !far {
            return Moved::Nothing;
        }
        live.points.push(point);
        Moved::Grew(Layer::of(live.semantic))
    }

    /// The release - over the pen layer or anywhere in the window - the pointer leaving the window,
    /// the window losing its focus: the stroke that is live ends. A tap is dropped (it cycles the nib,
    /// or does nothing while a dictation listens); a stroke is kept.
    pub fn stroke_end(&mut self) -> Option<Ended> {
        let done = self.live.take()?;
        let layer = Layer::of(done.semantic);
        let page = done.page;
        if done.points.len() < CLICK_POINT_LIMIT {
            // Dropped: the page's frame no longer shows the dab that was drawn. A kept stroke is
            // told to the page by `keep`: the frame already shows it.
            self.ink.touch(page);
            let up = if self.voice.running.is_some() {
                Up::Dab
            } else {
                Up::Click // RED STUB: every pointer cycles the nib
            };
            Some(Ended { layer, page, up })
        } else {
            self.keep(done);
            Some(Ended {
                layer,
                page,
                up: Up::Kept,
            })
        }
    }
}

// ==== The debug server ====

/// The debug server's op for the log: `azreview_pointer_log`.
pub const OP_NAME: &str = "azreview_pointer_log";

/// Adds the debug server's op to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"the last pointer events AzReview saw\",\
         \"description\":\"A JSON array, oldest first: n, ms, kind, page, x, y, pressure, pressed, pen, \
         live (the points of the stroke that was live).\",\"args\":[],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str()))
        .into_result()
        .unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
}

extern "C" fn on_op(
    mut data: RefAny,
    _info: CallbackInfo,
    name: AzString,
    _args: AzString,
) -> CustomE2eOpResult {
    if name.as_str() != OP_NAME {
        return CustomE2eOpResult {
            handled: false,
            json: AzString::from(""),
        };
    }
    let events = data
        .downcast_ref::<AppState>()
        .map_or_else(|| "[]".to_string(), |s| s.pointer_log.json());
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(format!("{{\"success\":true,\"events\":{events}}}").as_str()),
    }
}

#[cfg(test)]
#[path = "pointer_test.rs"]
mod pointer_test;
