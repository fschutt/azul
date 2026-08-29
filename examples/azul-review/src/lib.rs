//! AzReview — ink-first code review.
//!
//! # The inversion
//!
//! Every code-review tool makes you produce STRUCTURE first: click a line,
//! type a comment. Drawing, where it exists at all, is decoration on top.
//! Reviewing on paper works the other way round — you mark the code, and the
//! structure (which lines, which file, what kind of remark) is something a
//! reader infers afterwards from where the ink sits.
//!
//! This app takes the paper order as the real one. **The ink is the source of
//! truth; findings are derived and re-derived from it.** Nothing in the UI can
//! create a finding directly, which is what keeps the two from ever
//! disagreeing.
//!
//! # Why it lives here
//!
//! It is also the stress test for the tablet surface, and it uses all of it at
//! once rather than one feature at a time:
//!
//! * `PenState` — pressure drives nib width, tilt elongates the dab, and
//!   flipping the stylus erases (`is_eraser`).
//! * The Wacom **PAD** — ExpressKeys select the semantic colour without
//!   leaving the page, and the ring scrolls. That producer landed on this
//!   branch; before it, `get_wacom_pad()` returned `None` on every platform.
//! * Touch — a finger draws with a default pressure, so the app is usable on a
//!   tablet with no stylus at all.
//! * Microphone — audio recorded while drawing is bound to the strokes made in
//!   that window. The terse mark is the headline; the spoken part is the
//!   reasoning nobody wants to write by hand.

use std::path::PathBuf;

use azul::prelude::*;
use azul::task::TerminateTimer;
use azul::time::SystemTimeDiff;

pub mod code;
pub mod ink;
pub mod model;
pub mod session;
pub mod ui;

use model::{Finding, Semantic, Stroke, Tool, VoiceClip};

/// Autosave lands here. A review is long and interruptible; losing ink to a
/// crash would make the tool untrustworthy for the one job it has.
pub(crate) fn scratch_dir() -> PathBuf {
    std::env::var("AZ_REVIEW_DIR")
        .map_or_else(|_| std::env::temp_dir().join("azreview"), PathBuf::from)
}

pub struct AppState {
    pub files: Vec<code::SourceFile>,
    /// Index into `files`, or `None` before anything is opened.
    pub current: Option<usize>,
    /// All ink for the CURRENT file, keyed by page inside the stroke itself.
    pub strokes: Vec<Stroke>,
    /// In-progress stroke, promoted into `strokes` on pen-up.
    pub live: Option<Stroke>,
    pub next_stroke_id: u64,
    pub active: Semantic,
    /// Which nib is in hand. Cycled by a left click so the hand never has to
    /// leave the page to change tools.
    pub tool: Tool,
    /// Findings derived from `strokes`. Never edited directly.
    pub findings: Vec<Finding>,
    pub recording: Option<VoiceClip>,
    /// Clips already closed. Without this a finished clip would be dropped on
    /// the next record toggle, and the audio for every remark but the last one
    /// would never reach the archive.
    pub clips: Vec<VoiceClip>,
    /// Samples in the live clip as of the last DOM build, so the meter can be
    /// redrawn on a stroke boundary rather than per audio frame.
    pub level_samples: usize,
    /// Leftmost sheet in the strip, written by the `VirtualView` callback -
    /// the only place in the app that sees the engine's scroll offset. Drives
    /// which number the page rail highlights.
    pub visible_page: usize,
    /// Which annotation burst new strokes join. Bumped by the idle timer.
    pub epoch: u64,
    /// The idle timer's id, kept so each pen-up can RESTART one timer instead
    /// of piling up a fresh one per stroke.
    pub idle_timer: TimerId,
    pub root: PathBuf,
    pub status: String,
    /// Pad ExpressKeys as of the last frame, so a press EDGE can be detected.
    /// Without this the held key would re-fire the colour change every event.
    pub last_pad_keys: u32,
}

impl AppState {
    pub(crate) fn file(&self) -> Option<&code::SourceFile> {
        self.current.and_then(|i| self.files.get(i))
    }

    /// Re-derive every finding from the ink.
    ///
    /// Called after any change to `strokes`. Cheap enough to run wholesale:
    /// a session is hundreds of strokes, not millions, and re-deriving
    /// wholesale is what guarantees the findings cannot drift from the ink.
    fn rederive(&mut self) {
        let Some(file) = self.file().cloned() else {
            self.findings.clear();
            return;
        };
        self.findings = derive_findings(&self.strokes, &file, &self.recording);
    }
}

/// Cluster strokes into findings.
///
/// # The epoch does the work
///
/// Two marks are one remark when they were MADE as one remark, and the only
/// honest record of that is the pause between them — which is why the boundary
/// comes from the idle timer (`ANNOTATION_IDLE_MS`) and not from geometry.
/// Position alone cannot tell a second thought about a line from a correction
/// of the first, and both happen constantly.
///
/// Line proximity is still required WITHIN an epoch, because one uninterrupted
/// burst of marking often covers several unrelated places on a page.
fn derive_findings(
    strokes: &[Stroke],
    file: &code::SourceFile,
    recording: &Option<VoiceClip>,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for s in strokes {
        let (_, y0, _, y1) = s.bounds();
        // Which lines does the ink physically cover? This is the whole of
        // "lazy anchoring" — the user never states a range.
        let (first_line, _) = file.page(s.page);
        let l0 = first_line + (y0 / ui::LINE_H).floor().max(0.0) as usize;
        let l1 = first_line + (y1 / ui::LINE_H).floor().max(0.0) as usize;

        let merged = out.iter_mut().find(|f| {
            f.epoch == s.epoch
                && f.semantic == s.semantic
                && l0 <= f.last_line.saturating_add(2)
                && l1.saturating_add(2) >= f.first_line
        });
        if let Some(f) = merged {
            f.first_line = f.first_line.min(l0);
            f.last_line = f.last_line.max(l1);
            f.stroke_count += 1;
            continue;
        }
        let voice = recording
            .as_ref()
            .filter(|c| c.stroke_ids.contains(&s.id))
            .map(|c| format!("{} samples of spoken rationale", c.samples.len()));
        out.push(Finding {
            semantic: s.semantic,
            file: file.display.clone(),
            first_line: l0,
            last_line: l1,
            voice_note: voice,
            stroke_count: 1,
            epoch: s.epoch,
        });
    }
    out.sort_by_key(|f| f.first_line);
    out
}

// ───────── entry point ─────────

pub fn run() {
    let root = std::env::args().nth(1).map_or_else(
        || std::env::current_dir().unwrap_or_default(),
        PathBuf::from,
    );
    let files = code::load_tree(&root, 400);
    let status = if files.is_empty() {
        format!("no reviewable files under {}", root.display())
    } else {
        format!("{} files — pick one to start", files.len())
    };
    let state = AppState {
        current: if files.is_empty() { None } else { Some(0) },
        files,
        strokes: Vec::new(),
        live: None,
        next_stroke_id: 1,
        active: Semantic::Scope,
        tool: Tool::Marker,
        findings: Vec::new(),
        recording: None,
        clips: Vec::new(),
        level_samples: 0,
        visible_page: 0,
        epoch: 0,
        idle_timer: TimerId::unique(),
        root,
        status,
        last_pad_keys: 0,
    };
    let data = RefAny::new(state);
    let app = App::create(data, AppConfig::create());
    let mut window = WindowCreateOptions::create(ui::layout);
    window.window_state.title = "AzReview".into();
    app.run(window);
}

// ───────── input ─────────

/// Pen/touch down: start a stroke.
pub extern "C" fn on_ink_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(page) = ui::page_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((p, is_eraser)) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    if is_eraser {
        // Flipping the stylus erases the topmost stroke under the tip. Ink is
        // the source of truth, so erasing ink is how a finding is withdrawn —
        // there is no separate "delete finding" affordance to keep in sync.
        let hit = s.strokes.iter().rposition(|st| {
            let (x0, y0, x1, y1) = st.bounds();
            st.page == page
                && p.x >= x0 - 6.0
                && p.x <= x1 + 6.0
                && p.y >= y0 - 6.0
                && p.y <= y1 + 6.0
        });
        if let Some(i) = hit {
            s.strokes.remove(i);
            s.rederive();
            return Update::RefreshDom;
        }
        return Update::DoNothing;
    }
    let id = s.next_stroke_id;
    s.next_stroke_id += 1;
    // The MARKER can only ever mean "region": a fat translucent nib points at
    // code, it does not say anything about it. Letting it carry `issue` would
    // produce findings whose ink shape contradicts their label.
    let semantic = s.tool.semantic_for(s.active);
    let epoch = s.epoch;
    s.live = Some(Stroke {
        page,
        semantic,
        points: vec![p],
        id,
        epoch,
    });

    // The audio pen starts recording by being used. Any other way round means
    // remembering to arm it first, and the remark worth saying out loud is the
    // one made without stopping to think about the tool.
    if s.tool.records_audio() && s.recording.is_none() {
        s.recording = Some(VoiceClip {
            sample_rate: 48_000,
            ..VoiceClip::default()
        });
    }
    if let Some(clip) = s.recording.as_mut() {
        clip.stroke_ids.push(id);
    }
    Update::DoNothing
}

/// Left click with no drag cycles the nib.
///
/// On paper the two-layer grammar came from physically swapping pens, and the
/// swap is the friction: one goes down before the other comes up. A click that
/// changes tool without moving the hand is the whole reason this is not a
/// toolbar button.
pub extern "C" fn on_cycle_tool(mut data: RefAny, _: CallbackInfo) -> Update {
    cycle_tool(&mut data, false)
}

/// Right click / stylus barrel button: cycle the nib BACKWARD.
///
/// The barrel button reaches this as a right click on every backend (X11
/// maps the wacom barrel to button 3; the Wayland tablet bridge mirrors
/// `BTN_STYLUS` as the pointer's right button), so "left click forward,
/// right click back" is equally true for a mouse and for the pen in hand.
pub extern "C" fn on_cycle_tool_back(mut data: RefAny, _: CallbackInfo) -> Update {
    cycle_tool(&mut data, true)
}

fn cycle_tool(data: &mut RefAny, backward: bool) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.tool = if backward { s.tool.prev() } else { s.tool.next() };
    // Leaving the audio pen closes the clip rather than leaving it open: the
    // binding is to the strokes drawn WITH that nib, and a clip that kept
    // running would bind speech to marks made by a different tool.
    if !s.tool.records_audio() {
        if let Some(clip) = s.recording.take() {
            s.clips.push(clip);
        }
    }
    s.status = format!("{} - {}", s.tool.label(), s.active.label());
    Update::RefreshDom
}

/// Pen/touch move: extend the live stroke.
pub extern "C" fn on_ink_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.live.is_none() {
        return Update::DoNothing;
    }
    let Some((p, _)) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    if let Some(live) = s.live.as_mut() {
        // Drop samples that did not move: the pointer reports at a fixed rate
        // even when the pen is still, and a pile of coincident dabs is both
        // wasted raster work and a source of blotching.
        let far = live
            .points
            .last()
            .is_none_or(|l| (l.x - p.x).abs() + (l.y - p.y).abs() > 0.35);
        if far {
            live.points.push(p);
        }
    }
    // Repaint without rebuilding the DOM — the ink layer is an image callback.
    Update::RefreshDom
}

/// Pen/touch up: commit the stroke, or - if nothing was drawn - cycle the nib.
///
/// A press-and-release that never moved is a CLICK, and a click cycles the
/// tool. That overload is the point: swapping pens on paper means putting one
/// down to pick another up, and a tool change that costs a trip to a toolbar
/// reproduces exactly that interruption. A one-dab stroke would be invisible
/// anyway, so nothing is lost by spending it.
pub extern "C" fn on_ink_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (is_click, timer_id) = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(done) = s.live.take() else {
            return Update::DoNothing;
        };
        if done.points.len() < CLICK_POINT_LIMIT {
            (true, s.idle_timer)
        } else {
            s.strokes.push(done);
            s.rederive();
            session::save(&s);
            (false, s.idle_timer)
        }
    };
    if is_click {
        return on_cycle_tool(data, info);
    }
    arm_idle_timer(&mut info, data, timer_id);
    Update::RefreshDom
}

/// How long the ink must stay still before the annotation is considered
/// finished.
///
/// Long enough to survive lifting the pen to look at the code, short enough
/// that two genuinely separate remarks are not welded together. It is a
/// judgement about pen-and-paper rhythm, not a tuning constant with a right
/// answer — but SOME threshold is required, because the alternative is
/// guessing from geometry, which cannot distinguish a correction from a new
/// thought about the same line.
const ANNOTATION_IDLE_MS: u64 = 1_800;

/// (Re)start the idle timer, so it always measures from the LAST stroke.
///
/// One timer id reused rather than a fresh one per stroke: a dense burst of
/// marking would otherwise leave dozens of pending timers, every one of which
/// would fire and split the annotation it was supposed to keep whole.
fn arm_idle_timer(info: &mut CallbackInfo, data: RefAny, id: TimerId) {
    info.remove_timer(id);
    let timer = Timer::create(
        data,
        TimerCallback {
            cb: on_annotation_idle,
            ctx: OptionRefAny::None,
        },
        info.get_system_time_fn(),
    )
    .with_delay(Duration::System(SystemTimeDiff::from_millis(
        ANNOTATION_IDLE_MS,
    )));
    info.add_timer(id, timer);
}

/// The pen has been still long enough: seal this annotation.
///
/// Sealing is what makes the next stroke a NEW remark rather than a
/// continuation. It also closes an open audio clip, because the spoken
/// rationale belongs to the marks that were on the page while it was being
/// said — letting it run into the next annotation would bind it to ink it has
/// nothing to do with.
pub extern "C" fn on_annotation_idle(
    mut data: RefAny,
    _: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        };
    };
    s.epoch += 1;
    if let Some(clip) = s.recording.take() {
        s.clips.push(clip);
        s.level_samples = 0;
    }
    s.status = format!("annotation {} sealed", s.epoch);
    session::save(&s);
    TimerCallbackReturn {
        // One-shot: the next pen-up arms a fresh one. A repeating timer would
        // keep bumping the epoch through an idle session and scatter later ink
        // across epochs nobody ever drew in.
        should_update: Update::RefreshDom,
        should_terminate: TerminateTimer::Terminate,
    }
}

/// Below this many samples the gesture was a click, not a mark.
///
/// Not 1: a real dab jitters, and a stylus reports a sample or two before the
/// hand starts moving. Two points is still under a pixel of travel (`on_ink_move`
/// already drops samples closer than 0.35px), so this cannot eat a stroke
/// anyone meant to draw.
const CLICK_POINT_LIMIT: usize = 3;

/// The Wacom PAD, polled on every pointer event.
///
/// This is the producer that landed on this branch. ExpressKeys pick the
/// semantic colour so the palette never has to be visited with the pen, which
/// is the single biggest interruption in a paper review — putting one pen down
/// to pick another up.
pub extern "C" fn on_pad(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(pad) = info.get_wacom_pad().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    // Edge-triggered: a held key must not re-fire every event.
    let pressed = pad.express_keys & !s.last_pad_keys;
    s.last_pad_keys = pad.express_keys;
    if pressed == 0 {
        return Update::DoNothing;
    }
    let index = pressed.trailing_zeros() as usize;
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.active = sem;
        s.status = format!("pad key {index} -> {}", sem.label());
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// A page number in the rail: scroll that sheet to the left edge.
///
/// Scrolls rather than jumps because the strip is one continuous surface: an
/// instant teleport across forty sheets destroys the sense of WHERE page 50 is
/// relative to page 12, which is the only thing the rail exists to preserve.
pub extern "C" fn on_jump_to_page(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(page) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    ui::scroll_to_page(&mut info, page);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    // Highlight the target immediately. The VirtualView will confirm it on its
    // next invoke; without this the rail would not move until the strip did.
    s.visible_page = page;
    Update::RefreshDom
}

/// Ink menu → pick a semantic.
///
/// Separate from `on_pick_semantic` because a menu item carries its OWN
/// `RefAny` payload rather than a node dataset: there is no hit node to read an
/// `IndexTag` off, so the index arrives as the callback's data and the app
/// state has to be reached some other way.
pub extern "C" fn on_menu_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(tag) = data.downcast_ref::<ui::IndexTag>().map(|t| t.index) else {
        return Update::DoNothing;
    };
    let Some(&sem) = Semantic::ALL.get(tag) else {
        return Update::DoNothing;
    };
    let Some(mut app) = info.get_dataset(info.get_hit_node()).into_option() else {
        // The menu is on the DOM ROOT, so a menu click has no meaningful hit
        // node to carry state — see `on_menu_save` for why the other menu
        // items get the app handle directly instead.
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.active = sem;
    Update::RefreshDom
}

/// Session menu → write the archive now.
pub extern "C" fn on_menu_save(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.status = if session::save(&s) {
        format!("saved to {}", scratch_dir().display())
    } else {
        "save FAILED".to_string()
    };
    Update::RefreshDom
}

/// Session menu → open the archive folder in the desktop file manager.
pub extern "C" fn on_menu_reveal(mut data: RefAny, _: CallbackInfo) -> Update {
    let dir = scratch_dir();
    let _ = std::fs::create_dir_all(&dir);
    // Shelling out rather than going through a toolkit API because there is no
    // "reveal in file manager" in the C API to go through — noted rather than
    // hidden, since everything else in this app deliberately does.
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(&dir).spawn();

    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.status = format!("archives in {}", dir.display());
    Update::RefreshDom
}

/// Toolbar colour pick.
pub extern "C" fn on_pick_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.active = sem;
    }
    Update::RefreshDom
}

/// File browser pick — switches file and clears the page's ink from view.
pub extern "C" fn on_pick_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if index >= s.files.len() {
        return Update::DoNothing;
    }
    session::save(&s);
    s.current = Some(index);
    // Ink is per-file; switching files parks the current sheet rather than
    // carrying its marks onto unrelated code.
    s.strokes.clear();
    s.live = None;
    s.rederive();
    s.status = s.files[index].display.clone();
    Update::RefreshDom
}

/// Start/stop voice capture. While recording, every stroke id is appended to
/// the clip, which is what binds spoken rationale to specific ink.
///
/// A closed clip is PARKED, never dropped: it is already bound to strokes that
/// are still on the page, and losing it would leave those marks claiming a
/// voice note the archive does not contain.
pub extern "C" fn on_toggle_record(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if let Some(clip) = s.recording.take() {
        s.status = format!("recording stopped - {} samples kept", clip.samples.len());
        s.clips.push(clip);
        s.level_samples = 0;
        session::save(&s);
    } else {
        s.recording = Some(VoiceClip {
            sample_rate: 48_000,
            ..VoiceClip::default()
        });
        s.status = "recording - strokes drawn now carry this audio".to_string();
    }
    Update::RefreshDom
}

/// Microphone frames while recording.
pub extern "C" fn on_audio_frame(
    mut data: RefAny,
    _: CallbackInfo,
    frame: azul::widgets::AudioFrame,
) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(clip) = s.recording.as_mut() else {
        return Update::DoNothing;
    };
    clip.samples.extend(frame.samples.as_ref().iter().copied());
    let total = clip.samples.len();

    // Audio arrives at ~100 Hz. Repainting per frame would spend the whole
    // budget on a meter, so the bar advances only when a packet crosses a
    // visible step - which is exactly when the bar would change anyway.
    let step = ui::METER_PACKET_SAMPLES;
    if total / step > s.level_samples / step {
        s.level_samples = total;
        return Update::RefreshDom;
    }
    s.level_samples = total;
    Update::DoNothing
}
