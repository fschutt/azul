//! A fake AzReview: what the Mac side of the conversation knows and does, enough to drive the phone's screens in the
//! simulator and in tests. It holds a workspace to browse, the file under review with its pages, the viewport and the
//! pen (which can wander on its own), the four quick buttons, and the conversations a button press starts (received,
//! working, a suggestion waiting, done: the blue, yellow and green of AzReview's bottom bar). It writes down every command
//! it gets. It is the ONE place the simulator makes things up; the phone's screens and the protocol are the real ones.

use std::collections::BTreeMap;

use azul_review_link::frame::{
    is_safe_relative_path, ButtonSet, Context, Entry, EntryKind, FileState, Frame, PenMark, PenState, Phase, Rect, Reply, ReplyStatus,
    Rows, ThumbFormat, Thumbnail, ViewportOp,
};
use azul_review_link::limits::{Capped, MAX_ROWS};
use azul_review_link::link::Millis;
use azul_review_link::pace::Pacer;
use azul_review_link::viewport::{apply_gesture, fit_width, reset, resolve_focus};

/// The lines the log keeps.
pub const LOG_LINES: usize = 200;

/// The picture of a page: raw pixels, small enough for one frame.
pub const THUMB_W: u16 = 144;
/// See [`THUMB_W`].
pub const THUMB_H: u16 = 204;

/// How a conversation goes, from its start: received at once, an agent working after 0.3 s, a suggestion waiting after
/// 4 s, done after 9 s.
pub const STAGES: [(Millis, ReplyStatus); 4] = [
    (0, ReplyStatus::Received),
    (300, ReplyStatus::Working),
    (4_000, ReplyStatus::Suggested),
    (9_000, ReplyStatus::Done),
];

/// A conversation a button started.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    pub request_id: u32,
    pub button: usize,
    pub path: String,
    pub started: Millis,
    /// How many of the [`STAGES`] the phone has been told.
    pub stage: usize,
}

/// A line of the log as the trace prints it: the time in seconds, then the words, on one line.
#[must_use]
pub fn trace_line(at: Millis, text: &str) -> String {
    let one_line: String = text.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
    format!("[trace] {:.3} {one_line}", at as f64 / 1000.0)
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub at: Millis,
    pub text: String,
}

/// The fake AzReview.
#[derive(Debug)]
pub struct SimMac {
    pub name: String,
    /// AzReview is running (the phone can find it).
    pub online: bool,
    tree: BTreeMap<String, Vec<Entry>>,
    pub file: String,
    pub page: u32,
    pub page_count: u32,
    pub line: u32,
    pub view: Rect,
    pub page_aspect: f32,
    pub pen: Option<PenMark>,
    /// The pen walks over the page by itself.
    pub pen_walk: bool,
    pub buttons: ButtonSet,
    pub conversations: Vec<Conversation>,
    pub log: Vec<LogLine>,
    seq: u32,
    thumb_id: u32,
    thumb_sent: u32,
    gesture: Option<(u32, Rect, (f32, f32))>,
    pen_pacer: Pacer<PenState>,
    last_sent: Option<PenState>,
    walk_phase: f32,
    last_tick: Option<Millis>,
    last_gesture_log: Millis,
    buttons_sent: bool,
    /// The user marked a region with the pen: the buttons act on it.
    pub armed_region: bool,
    /// Every line of the log is also printed (for a script that drives the simulator).
    pub trace: bool,
}

/// The files and folders of the workspace the simulator browses (a folder's name ends with `/`).
const FIXTURE: &[(&str, &[&str])] = &[
    ("", &["core/", "css/", "dll/", "doc/", "examples/", "layout/", "scripts/", "tests/", "Cargo.toml", "README.md", "api.json", "LICENSE", "rust-toolchain.toml"]),
    ("core", &["src/", "tests/", "Cargo.toml"]),
    ("core/src", &["callbacks.rs", "dom.rs", "events.rs", "gl.rs", "id.rs", "lib.rs", "resources.rs", "styled_dom.rs", "window.rs", "xml.rs"]),
    ("core/tests", &["events.rs", "hit_test.rs"]),
    ("css", &["src/", "Cargo.toml"]),
    ("css/src", &["lib.rs", "parser.rs", "props/", "system.rs"]),
    ("css/src/props", &["basic.rs", "layout.rs", "style.rs"]),
    ("dll", &["src/", "build.rs", "Cargo.toml"]),
    ("dll/src", &["desktop/", "lib.rs", "unified.rs"]),
    ("dll/src/desktop", &["extra/", "shell2/", "mod.rs"]),
    ("dll/src/desktop/extra", &["iroh/", "pdf/", "mod.rs"]),
    ("dll/src/desktop/shell2", &["android/", "ios/", "linux/", "macos/", "windows/", "run.rs"]),
    ("doc", &["guide/", "src/", "Cargo.toml"]),
    ("examples", &["azul-companion/", "azul-maps/", "azul-meet/", "azul-paint/", "azul-review/", "azul-review-link/"]),
    ("examples/azul-review", &["resources/", "src/", "Cargo.toml", "FLOW.md", "RECORDER.md"]),
    ("examples/azul-review/src", &["column.rs", "ink.rs", "layout.rs", "lib.rs", "model.rs", "pens.rs", "ui.rs"]),
    ("layout", &["src/", "tests/", "Cargo.toml"]),
    ("layout/src", &["managers/", "solver3/", "text3/", "widgets/", "callbacks.rs", "cpurender.rs", "window.rs"]),
    ("layout/src/managers", &["focus.rs", "gesture.rs", "scroll.rs"]),
    ("layout/src/solver3", &["cache.rs", "fc.rs", "layout_tree.rs", "mod.rs", "positioning.rs", "sizing.rs"]),
    ("layout/src/text3", &["cache.rs", "default.rs", "glyphs.rs", "mod.rs"]),
    ("layout/src/widgets", &["button.rs", "map.rs", "text_input.rs"]),
    ("layout/tests", &["reftest.rs", "synthetic_events.rs"]),
    ("scripts", &["build-android.sh", "build-ios.sh", "remote.sh"]),
    ("tests", &["e2e/", "fixtures/"]),
    ("tests/e2e", &["op-scroll-to-position.json", "pinch-zoom.json"]),
];

/// How many entries the long folder of the workspace has (more than a frame holds).
const FIXTURE_LONG: usize = 700;
/// The folder that has that many.
const FIXTURE_LONG_DIR: &str = "tests/fixtures";

fn hash(text: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The colour a file has in the tree before any conversation (a made-up spread over the workspace).
fn base_state(path: &str) -> FileState {
    match hash(path) % 100 {
        0..=51 => FileState::None,
        52..=73 => FileState::Clean,
        74..=87 => FileState::Unresolved,
        _ => FileState::Working,
    }
}

fn rank(state: FileState) -> u8 {
    match state {
        FileState::None => 0,
        FileState::Clean => 1,
        FileState::Unresolved => 2,
        FileState::Working => 3,
    }
}

impl SimMac {
    /// A Mac with a workspace open on `layout/src/solver3/fc.rs`, page 3 of its pages.
    #[must_use]
    pub fn new() -> SimMac {
        let mut tree: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
        for (dir, names) in FIXTURE {
            let mut entries: Vec<Entry> = names
                .iter()
                .map(|n| match n.strip_suffix('/') {
                    Some(folder) => Entry { name: folder.to_string(), kind: EntryKind::Dir, state: FileState::None },
                    None => Entry { name: (*n).to_string(), kind: EntryKind::File, state: FileState::None },
                })
                .collect();
            entries.sort_by(|a, b| {
                (a.kind == EntryKind::File, a.name.to_lowercase()).cmp(&(b.kind == EntryKind::File, b.name.to_lowercase()))
            });
            tree.insert((*dir).to_string(), entries);
        }
        tree.insert(
            FIXTURE_LONG_DIR.to_string(),
            (1..=FIXTURE_LONG)
                .map(|i| Entry { name: format!("case_{i:04}.json"), kind: EntryKind::File, state: FileState::None })
                .collect(),
        );
        let mut mac = SimMac {
            name: "Studio Mac".to_string(),
            online: true,
            tree,
            file: "layout/src/solver3/fc.rs".to_string(),
            page: 2,
            page_count: 9,
            line: 1337,
            view: SimMac::default_view(),
            page_aspect: 794.0 / 1123.0,
            pen: Some(PenMark { x: 0.34, y: 0.21, down: false }),
            pen_walk: false,
            buttons: ButtonSet::defaults(),
            conversations: Vec::new(),
            log: Vec::new(),
            seq: 0,
            thumb_id: 1,
            thumb_sent: 0,
            gesture: None,
            pen_pacer: Pacer::new(30),
            last_sent: None,
            walk_phase: 0.0,
            last_tick: None,
            last_gesture_log: 0,
            buttons_sent: false,
            armed_region: false,
            trace: false,
        };
        mac.page_count = mac.pages_of(&mac.file);
        mac.page = 2;
        mac
    }

    /// What the Mac shows of a page that was just opened: the top left, a little over a half of the width.
    fn default_view() -> Rect {
        Rect { x: 0.0, y: 0.0, w: 0.625, h: 0.4 }
    }

    /// How many pages the file has (3 to 12, by its name).
    fn pages_of(&self, path: &str) -> u32 {
        3 + (hash(path) % 10) as u32
    }

    /// Whether `path` is a file of the workspace.
    fn is_file(&self, path: &str) -> bool {
        if !is_safe_relative_path(path) || path.is_empty() {
            return false;
        }
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        self.tree.get(dir).is_some_and(|entries| entries.iter().any(|e| e.kind == EntryKind::File && e.name == name))
    }

    /// The rows of `path` from `offset` on (a folder longer than a frame comes in pages).
    #[must_use]
    pub fn list(&self, request_id: u32, path: &str, offset: u32) -> Rows {
        let all: Vec<Entry> = if is_safe_relative_path(path) {
            self.tree
                .get(path)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|e| Entry { state: self.state_of(&join(path, &e.name)), ..e.clone() })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let start = (offset as usize).min(all.len());
        let end = (start + MAX_ROWS).min(all.len());
        Rows { request_id, dir: path.to_string(), offset, entries: all[start..end].to_vec(), more: end < all.len() }
    }

    /// AzReview opens `path` at `line` (the first unreviewed line when none is given).
    pub fn open_file(&mut self, now: Millis, path: &str, line: Option<u32>) {
        if !self.is_file(path) {
            self.say(now, format!("refused to open {path:?}: not a file of the workspace"));
            return;
        }
        self.file = path.to_string();
        self.page = 0;
        self.page_count = self.pages_of(path);
        self.line = line.unwrap_or(1 + ((hash(path) >> 8) % 400) as u32);
        self.view = SimMac::default_view();
        self.gesture = None;
        self.thumb_id += 1;
        self.say(now, format!("open {path}:{}", self.line));
    }

    /// The phone's viewport command.
    pub fn apply(&mut self, now: Millis, op: ViewportOp) {
        match op {
            ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus } => {
                let (base, about) = match self.gesture {
                    Some((current, base, about)) if current == id => (base, about),
                    _ => {
                        // the first sight of a gesture is its start, whatever frame it came in
                        let base = self.view;
                        let about = resolve_focus(focus, base, self.pen.map(|p| (p.x, p.y)));
                        self.gesture = Some((id, base, about));
                        (base, about)
                    }
                };
                self.view = apply_gesture(base, (pan_x, pan_y), zoom, about);
                let note = match phase {
                    Phase::Begin => Some("begin"),
                    Phase::End => Some("end"),
                    Phase::Update if now >= self.last_gesture_log + 250 => Some("update"),
                    Phase::Update => None,
                };
                if let Some(note) = note {
                    self.last_gesture_log = now;
                    self.say(now, format!("pan {note} #{id}  {pan_x:+.2} {pan_y:+.2}  x{zoom:.2}"));
                }
                if phase == Phase::End {
                    self.gesture = None;
                }
            }
            ViewportOp::Fit => {
                self.view = fit_width(self.view);
                self.say(now, "fit width".to_string());
            }
            ViewportOp::Reset => {
                self.view = reset(self.view);
                self.say(now, "reset view".to_string());
            }
            ViewportOp::PageStep(step) => {
                let target = (i64::from(self.page) + i64::from(step)).clamp(0, i64::from(self.page_count.saturating_sub(1))) as u32;
                if target != self.page {
                    self.page = target;
                    self.thumb_id += 1;
                    self.view.y = 0.0;
                    self.view = azul_review_link::viewport::clamp_view(self.view);
                }
                self.say(now, format!("page {} of {}", self.page + 1, self.page_count));
            }
        }
    }

    /// A press of quick button `index`: a conversation starts.
    pub fn press_button(&mut self, now: Millis, index: usize, request_id: u32) {
        let Some(button) = self.buttons.buttons.get(index) else {
            self.say(now, format!("button {index}: no such button"));
            return;
        };
        let text = format!("button {} \"{}\": {} [{}]", index + 1, button.label, button.text, self.context_words());
        self.say(now, text);
        self.conversations.push(Conversation { request_id, button: index, path: self.file.clone(), started: now, stage: 0 });
    }

    /// The phone saved the four buttons: what AzReview keeps, and what it cut.
    pub fn set_buttons(&mut self, now: Millis, set: &ButtonSet) -> Vec<Capped> {
        let (clean, caps) = set.sanitized();
        self.buttons = clean;
        let cut = if caps.is_empty() { String::new() } else { format!(" (cut: {})", caps.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")) };
        self.say(now, format!("buttons saved{cut}"));
        caps
    }

    /// What the context at the pen is now.
    #[must_use]
    pub fn context(&self) -> Context {
        if self.armed_region {
            Context::Region
        } else if self.pen.is_some() && !self.file.is_empty() {
            Context::Line
        } else {
            Context::Page
        }
    }

    fn context_words(&self) -> String {
        match self.context() {
            Context::Region => "the marked region".to_string(),
            Context::Line => format!("line {}", self.line),
            Context::Page => format!("page {}", self.page + 1),
        }
    }

    /// The pen state as it is now, without counting up.
    fn snapshot(&self) -> PenState {
        PenState {
            seq: 0,
            folder: "azul".to_string(),
            file: self.file.clone(),
            page_index: self.page,
            page_count: if self.file.is_empty() { 0 } else { self.page_count },
            line: if self.file.is_empty() { 0 } else { self.line },
            line_count: if self.file.is_empty() { 0 } else { 600 + (hash(&self.file) % 3000) as u32 },
            page_w: 794.0,
            page_h: 794.0 / self.page_aspect,
            view: self.view,
            zoom: azul_review_link::viewport::zoom_of(self.view),
            pen: self.pen,
            thumb_id: if self.file.is_empty() { 0 } else { self.thumb_id },
            context: self.context(),
        }
    }

    /// The pen state as it is now (a new one every call counts up).
    pub fn pen_state(&mut self) -> PenState {
        self.seq = self.seq.wrapping_add(1);
        PenState { seq: self.seq, ..self.snapshot() }
    }

    /// The picture of the page on screen.
    #[must_use]
    pub fn thumbnail(&self) -> Thumbnail {
        Thumbnail {
            id: self.thumb_id,
            width: THUMB_W,
            height: THUMB_H,
            format: ThumbFormat::Rgba8,
            bytes: render_page(hash(&self.file) ^ (u64::from(self.page) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
        }
    }

    /// Time passes: the frames AzReview sends the phone (pen states at most 30 a second, the picture when the page
    /// changed, the replies of the conversations).
    pub fn tick(&mut self, now: Millis) -> Vec<Frame> {
        let mut out = Vec::new();
        let dt = self.last_tick.map_or(0, |t| now.saturating_sub(t)).min(100);
        self.last_tick = Some(now);
        if self.pen_walk {
            self.walk_phase += dt as f32 * 0.0011;
            let p = self.walk_phase;
            let v = self.view;
            let (fx, fy) = (0.5 + 0.38 * (p * 1.3).sin(), 0.5 + 0.34 * (p * 0.9 + 1.0).sin());
            let down = (p * 0.5).sin() > 0.55;
            self.pen = Some(PenMark { x: (v.x + v.w * fx).clamp(0.0, 1.0), y: (v.y + v.h * fy).clamp(0.0, 1.0), down });
            self.line = 1 + ((self.pen.map_or(0.0, |m| m.y) + self.page as f32) * 55.0) as u32;
        }
        for conv in &mut self.conversations {
            let elapsed = now.saturating_sub(conv.started);
            let due = STAGES.iter().filter(|(at, _)| elapsed >= *at).count();
            while conv.stage < due {
                out.push(Frame::Reply(Reply { request_id: conv.request_id, status: STAGES[conv.stage].1 }));
                conv.stage += 1;
            }
        }
        if !self.buttons_sent {
            self.buttons_sent = true;
            out.push(Frame::Buttons(self.buttons.clone()));
        }
        if self.thumb_sent != self.thumb_id && !self.file.is_empty() {
            self.thumb_sent = self.thumb_id;
            out.push(Frame::Thumbnail(self.thumbnail()));
        }
        let snapshot = self.snapshot();
        let changed = match (&self.last_sent, self.pen_pacer.has_pending()) {
            (Some(sent), false) => *sent != snapshot,
            (None, _) | (_, true) => true,
        };
        if changed {
            self.pen_pacer.offer(snapshot.clone());
        }
        if let Some(due) = self.pen_pacer.take(now) {
            self.seq = self.seq.wrapping_add(1);
            self.last_sent = Some(due.clone());
            out.push(Frame::PenState(PenState { seq: self.seq, ..due }));
        }
        out
    }

    /// The link came back: the next tick says everything again (the buttons, the picture, the pen state).
    pub fn reset_sent(&mut self) {
        self.thumb_sent = 0;
        self.last_sent = None;
        self.buttons_sent = false;
        self.pen_pacer.clear();
    }

    /// Writes a line in the log.
    pub fn say(&mut self, now: Millis, text: String) {
        if self.trace {
            println!("{}", trace_line(now, &text));
        }
        self.log.push(LogLine { at: now, text });
        if self.log.len() > LOG_LINES {
            let extra = self.log.len() - LOG_LINES;
            self.log.drain(..extra);
        }
    }

    /// The state AzReview colours a path in its tree.
    #[must_use]
    pub fn state_of(&self, path: &str) -> FileState {
        if let Some(conv) = self.conversations.iter().rev().find(|c| c.path == path) {
            return match conv.stage {
                0..=2 => FileState::Working,
                3 => FileState::Unresolved,
                _ => FileState::Clean,
            };
        }
        match self.tree.get(path) {
            Some(children) => children
                .iter()
                .map(|e| self.state_of(&join(path, &e.name)))
                .fold(FileState::None, |a, b| if rank(b) > rank(a) { b } else { a }),
            None => base_state(path),
        }
    }
}

/// A page of code at the size of a thumbnail: paper, a gutter of line numbers, lines of text with an accent at the
/// start of some (a keyword), blank lines between blocks. The same `seed` always draws the same page.
fn render_page(seed: u64) -> Vec<u8> {
    let (w, h) = (usize::from(THUMB_W), usize::from(THUMB_H));
    let mut px = vec![0u8; w * h * 4];
    let mut put = |x: usize, y: usize, c: [u8; 3]| {
        if x < w && y < h {
            let i = (y * w + x) * 4;
            px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    };
    for y in 0..h {
        for x in 0..w {
            put(x, y, [251, 250, 246]);
        }
    }
    for x in 0..w {
        put(x, 0, [226, 225, 220]);
        put(x, h - 1, [226, 225, 220]);
    }
    for y in 0..h {
        put(0, y, [226, 225, 220]);
        put(w - 1, y, [226, 225, 220]);
    }
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33
    };
    let accents: [[u8; 3]; 3] = [[150, 60, 200], [32, 92, 214], [24, 140, 70]];
    let mut indent = 0usize;
    let mut y = 12;
    while y + 3 < h - 8 {
        let kind = next() % 10;
        if kind == 0 {
            y += 5; // a blank line between blocks
            continue;
        }
        // the gutter's line number
        for x in 5..9 {
            put(x, y, [186, 186, 192]);
        }
        match kind {
            1 | 2 => indent = indent.saturating_sub(5),
            3 | 4 => indent = (indent + 5).min(20),
            _ => {}
        }
        let start = 14 + indent;
        let len = 14 + (next() % 70) as usize;
        let end = (start + len).min(w - 10);
        let accent = if next() % 4 == 0 { Some(accents[(next() % 3) as usize]) } else { None };
        for x in start..end {
            let colour = match accent {
                Some(c) if x < start + 10 => c,
                _ => [58, 58, 64],
            };
            put(x, y, colour);
            put(x, y + 1, colour);
        }
        y += 5;
    }
    px
}

impl Default for SimMac {
    fn default() -> Self {
        SimMac::new()
    }
}

#[cfg(test)]
#[path = "mac_test.rs"]
mod mac_test;
