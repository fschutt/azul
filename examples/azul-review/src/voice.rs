//! The dictation on the window: the pen's rest becomes a note that listens.
//!
//! The pointer's samples go to [`crate::dwell::Dwell`] (pen at rest over a page); a timer ticks it
//! while a rest is being timed; a rest that lasts starts a dictation at the pen - a note, a marker on
//! the page, a worker thread that listens call after call ([`crate::dictation::run`]) and hands the
//! words back to the window as they come. The pen moving away, drawing, resting again, the stop button,
//! Escape, a stretch of quiet or another file ends it. What the user said stays in the note, which is
//! theirs to edit and is kept in the session archive.
//!
//! stdout, for scripts (`scripts/azreview_e2e.py`): `AZREVIEW_DICTATION started <note>`, `text <note>
//! <words>`, `ended <note> <cause>`, `refused <status> <why>`.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration as StdDuration, Instant},
};

use azul::{
    option::OptionSpeechWord,
    prelude::*,
    speech::{BuiltinSpeechStatus, HeardStatus, Speech, SpeechHeard, SpeechRecognizer, SpeechSession},
    str::String as AzString,
    vec::StringVec,
    task::TerminateTimer,
    time::SystemTimeDiff,
    widgets::{OnTextInputReturn, TextAreaState, TextInputValid},
};
use azul_appkit::{
    l10n::{t_args, Arg},
    ui as kit,
};

use crate::{
    dictation::{self, Cause, Clock, End, Heard, HeardWord, Listener, Out, Plan, Recognizer, Status},
    dwell::{Allow, Anchor, Config, Dwell, Reaction, Sample},
    model::{Note, NoteState},
    ui, AppState,
};

/// How often a rest being timed is looked at, ms: the dwell fires at most this late.
const TICK_MS: u64 = 100;

/// A dictation that is running or finishing.
pub struct Running {
    /// Which dictation: the worker's messages carry it.
    pub session: u64,
    /// The note it writes.
    pub note: u64,
    /// The worker looks at this between two listen calls.
    stop: Arc<AtomicBool>,
    /// Why it was asked to stop (`None`: still listening). The worker finishes the call it is in
    /// first: what was said meanwhile is kept.
    pub stopping: Option<Cause>,
}

impl Running {
    /// A running dictation for a test: no worker behind it.
    #[cfg(test)]
    #[must_use]
    pub fn for_test(session: u64, note: u64) -> Self {
        Self {
            session,
            note,
            stop: Arc::new(AtomicBool::new(false)),
            stopping: None,
        }
    }
}

/// The dictation of one window: the dwell, the running session, the column's state.
pub struct Voice {
    pub dwell: Dwell,
    /// How long without words ends a dictation, ms.
    pub silence_ms: u64,
    clock: Instant,
    /// The tick timer is armed.
    pub ticking: bool,
    pub running: Option<Running>,
    next_session: u64,
    /// A rest reported while the last worker was still finishing: it starts when that one ends, if
    /// the pen is still resting there.
    pub pending: Option<Anchor>,
    /// The side column shows.
    pub notes_open: bool,
    /// Why speech recognition cannot be had at all here (no recogniser, the app not set up for one):
    /// it cannot change before the app is started again, so a rest says so again and starts nothing.
    pub unavailable: Option<String>,
    /// The speech session: the chain of recognisers (the system's, the built-in one), its vocabulary and the call
    /// that ends when a dictation is stopped. One for the window; the worker listens through a clone of it.
    pub speech: Option<SpeechSession>,
}

impl Voice {
    #[must_use]
    pub fn new(dwell: Config, silence_ms: u64) -> Self {
        Self {
            dwell: Dwell::new(dwell),
            silence_ms,
            clock: Instant::now(),
            ticking: false,
            running: None,
            next_session: 1,
            pending: None,
            notes_open: false,
            unavailable: None,
            speech: None,
        }
    }

    /// Milliseconds since the window's voice began: the dwell's clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Whether a dictation is listening (not already asked to stop).
    #[must_use]
    pub fn is_listening(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.stopping.is_none())
    }

    /// The note a running dictation writes (listening or finishing).
    #[must_use]
    pub fn running_note(&self) -> Option<u64> {
        self.running.as_ref().map(|r| r.note)
    }

    /// Asks the running dictation to stop; `false` when there is none, or it was asked already.
    pub fn request_stop(&mut self, cause: Cause) -> bool {
        let Some(r) = self.running.as_mut() else {
            return false;
        };
        if r.stopping.is_some() {
            return false;
        }
        r.stopping = Some(cause);
        r.stop.store(true, Ordering::Relaxed);
        // The call that listens ends now and answers what was said so far: the stop is heard at once, not after the
        // utterance.
        if let Some(speech) = &self.speech {
            speech.cancel();
        }
        self.dwell.session_ended();
        true
    }
}

// ==== The pointer ====

/// The pointer is over a page of the pen layer (a move, a press, a release): where it is goes to the
/// dwell, a running dictation the pen has left ends, and the tick timer is armed while a rest is being
/// timed. `down`: the pointer's button / the pen's tip, when the event says (a release is up);
/// else it is read from the window.
pub fn pointer(data: &mut RefAny, info: &mut CallbackInfo, down: Option<bool>) -> Update {
    let Some(page) = ui::page_of(info) else {
        return Update::DoNothing;
    };
    let Some(at) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let pen = info.get_pen_state().into_option();
    let contact = down.unwrap_or_else(|| match &pen {
        Some(p) => p.in_contact,
        None => info.get_current_mouse_state().left_down,
    });
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.voice.dwell.config().allow == Allow::Off && s.voice.running.is_none() {
        return Update::DoNothing;
    }
    let now = s.voice.now_ms();
    let sample = Sample {
        page,
        x: at.x,
        y: at.y,
        pen: pen.is_some(),
        contact,
    };
    let mut update = Update::DoNothing;
    if s.voice.dwell.observe(sample, now) == Reaction::MovedAway {
        let cause = if contact { Cause::Drew } else { Cause::Moved };
        if s.voice.request_stop(cause) {
            update = Update::RefreshDom;
        }
    }
    let arm = s.voice.dwell.is_waiting() && !s.voice.ticking;
    if arm {
        s.voice.ticking = true;
    }
    drop(s);
    if arm {
        let timer = Timer::create(data.clone(), on_dwell_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
        info.add_timer(TimerId::unique(), timer);
    }
    update
}

/// The pointer left the page (off it, off the window, or the pen out of the tablet's range).
pub extern "C" fn on_pointer_left(mut data: RefAny, _info: CallbackInfo) -> Update {
    pointer_left(&mut data)
}

/// What the pointer leaving the page does to the dwell and to a dictation: the rest is over, and a
/// dictation the pen has moved away from ends.
pub fn pointer_left(data: &mut RefAny) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.voice.dwell.left() == Reaction::MovedAway && s.voice.request_stop(Cause::Moved) {
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// The tick of a rest being timed: a rest that has lasted is a dwell - it starts a dictation, or (a
/// second dwell, while one listens) stops it.
extern "C" fn on_dwell_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let pen_here = info.callback_info.get_pen_state().into_option().is_some();
    let anchor = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return TimerCallbackReturn::terminate_unchanged();
        };
        let now = s.voice.now_ms();
        // A pen that went out of the tablet's range leaves the cursor where it was: no rest to time.
        if s.voice.dwell.config().allow == Allow::Pen && !pen_here {
            if s.voice.dwell.left() == Reaction::MovedAway {
                s.voice.request_stop(Cause::Moved);
            }
            s.voice.ticking = false;
            return TimerCallbackReturn::terminate_and_refresh_dom();
        }
        let anchor = s.voice.dwell.tick(now);
        if anchor.is_none() && !s.voice.dwell.is_waiting() {
            s.voice.ticking = false;
            return TimerCallbackReturn::terminate_unchanged();
        }
        anchor
    };
    let Some(anchor) = anchor else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let update = rested(&mut data, &mut info.callback_info, anchor);
    let keep = data
        .downcast_mut::<AppState>()
        .is_some_and(|mut s| {
            let waiting = s.voice.dwell.is_waiting();
            s.voice.ticking = waiting;
            waiting
        });
    TimerCallbackReturn::create(update, if keep { TerminateTimer::Continue } else { TerminateTimer::Terminate })
}

/// A rest was reported at `anchor`: a dictation starts there, or the one that listens stops (the second
/// dwell).
fn rested(data: &mut RefAny, info: &mut CallbackInfo, anchor: Anchor) -> Update {
    let action = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        if s.voice.is_listening() {
            s.voice.request_stop(Cause::SecondDwell);
            Some(false)
        } else if s.voice.running.is_some() {
            // The last worker is still finishing its call: this rest starts a dictation after it.
            s.voice.pending = Some(anchor);
            None
        } else {
            Some(true)
        }
    };
    match action {
        Some(true) => start(data, info, anchor),
        Some(false) => Update::RefreshDom,
        None => Update::DoNothing,
    }
}

// ==== The session ====

/// What the built-in recogniser's loading came to, in words (a line for the log and the status).
pub(crate) fn builtin_word(status: BuiltinSpeechStatus) -> &'static str {
    match status {
        BuiltinSpeechStatus::Ready => "ready",
        BuiltinSpeechStatus::EngineMissing => "its engine library was not found",
        BuiltinSpeechStatus::EngineIncompatible => "its engine library does not fit this app",
        BuiltinSpeechStatus::ModelMissing => "its speech model was not found",
        BuiltinSpeechStatus::ModelRejected => "its speech model was refused",
    }
}

/// How many words of the page the recogniser is told (its own cap is 200).
const VOCABULARY_CAP: usize = 200;

/// A list of words as the engine's string list.
fn string_vec(words: &[String]) -> StringVec {
    let items: Vec<AzString> = words.iter().map(|w| AzString::from(w.as_str())).collect();
    match items.first() {
        Some(first) => StringVec::copy_from_ptr(first, items.len()),
        None => StringVec::create(),
    }
}

/// What the worker thread starts from.
struct WorkerInit {
    plan: Plan,
    stop: Arc<AtomicBool>,
    session: u64,
    /// The window's speech session (a clone: the same session): without one the engine's static recogniser is asked.
    speech: Option<SpeechSession>,
}

/// What the worker hands to the window.
struct Message {
    session: u64,
    out: Out,
}

/// What a listen call of the engine answered, as the dictation sees it: the words with their times, the moment the
/// call began and who answered.
pub(crate) fn heard_of(heard: &SpeechHeard) -> Heard {
    let mut words = Vec::new();
    for i in 0..heard.words.len() {
        if let OptionSpeechWord::Some(w) = heard.words.c_get(i) {
            words.push(HeardWord {
                text: w.text.as_str().to_string(),
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                probability: w.probability,
            });
        }
    }
    Heard {
        status: match heard.status {
            HeardStatus::Heard => Status::Heard,
            HeardStatus::NothingHeard => Status::Nothing,
            HeardStatus::Denied => Status::Denied,
            HeardStatus::Unavailable => Status::Unavailable,
            HeardStatus::Failed => Status::Failed,
        },
        text: heard.text.as_str().to_string(),
        message: heard.message.as_str().to_string(),
        words,
        started_unix_ms: heard.started_unix_ms,
        recognizer: match heard.recognizer {
            SpeechRecognizer::Native => Recognizer::Native,
            SpeechRecognizer::Builtin => Recognizer::Builtin,
            SpeechRecognizer::Scripted => Recognizer::Scripted,
            SpeechRecognizer::None => Recognizer::None,
        },
    }
}

/// The engine's recogniser as a [`Listener`]: the window's session (the chain of recognisers, its vocabulary), or the
/// engine's static call where there is none.
struct SpeechListener {
    session: Option<SpeechSession>,
}

impl Listener for SpeechListener {
    fn listen(&mut self, language: &str, timeout_ms: u32) -> Heard {
        match &self.session {
            Some(session) => heard_of(&session.listen_blocking(language.to_string(), timeout_ms)),
            None => heard_of(&Speech::listen_blocking(language.to_string(), timeout_ms)),
        }
    }
}

/// The wall clock of a worker.
struct WallClock(Instant);

impl Clock for WallClock {
    fn now_ms(&mut self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(StdDuration::from_millis(ms));
    }
}

/// The worker thread: listens until it is told to stop, then says so.
extern "C" fn worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((plan, stop, session, speech)) = init
        .downcast_ref::<WorkerInit>()
        .map(|i| (i.plan.clone(), i.stop.clone(), i.session, i.speech.clone()))
    else {
        return;
    };
    dictation::run(
        &plan,
        &mut SpeechListener { session: speech },
        &mut WallClock(Instant::now()),
        &mut || stop.load(Ordering::Relaxed),
        &mut |out| {
            let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
                on_message,
                RefAny::new(Message { session, out }),
            )));
        },
    );
}

/// Starts a dictation at `anchor`: its note and marker, the worker, the column open.
fn start(data: &mut RefAny, info: &mut CallbackInfo, anchor: Anchor) -> Update {
    let plan = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        if s.file().is_none() {
            return Update::DoNothing;
        }
        if let Some(why) = s.voice.unavailable.clone() {
            // Nothing to listen with: say so again, over the pages, and start nothing.
            s.problem = Some(refusal(Status::Unavailable, &why));
            return Update::RefreshDom;
        }
        // The language of the app's words now (Settings > Language may have changed since the start).
        let language = language_of(&s.kit);
        let id = s.next_note_id;
        s.next_note_id += 1;
        s.notes.push(Note::listening(id, anchor.page, anchor.x, anchor.y, &language));
        let session = s.voice.next_session;
        s.voice.next_session += 1;
        let stop = Arc::new(AtomicBool::new(false));
        s.voice.running = Some(Running {
            session,
            note: id,
            stop: stop.clone(),
            stopping: None,
        });
        s.voice.dwell.session_started(anchor);
        s.voice.notes_open = true;
        println!("AZREVIEW_DICTATION started {id}");
        // The words of the page under the pen, for the recogniser's keyword biasing (what it cut is told once).
        if let Some(speech) = &s.voice.speech {
            let words = s.file().map_or_else(Vec::new, |f| {
                crate::vocabulary::vocabulary(f.page(anchor.page).1, &f.display, VOCABULARY_CAP)
            });
            let cut = speech.set_vocabulary(string_vec(&words));
            if !cut.as_str().is_empty() {
                eprintln!("[AzReview] speech vocabulary: {}", cut.as_str());
            }
        }
        (
            Plan {
                language,
                chunk_ms: dictation::CHUNK_MS,
                silence_ms: s.voice.silence_ms,
                min_gap_ms: dictation::MIN_GAP_MS,
            },
            stop,
            session,
            s.voice.speech.clone(),
        )
    };
    let (plan, stop, session, speech) = plan;
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(WorkerInit { plan, stop, session, speech }), data.clone(), worker),
    );
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// What the worker handed over, on the UI thread.
extern "C" fn on_message(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some((session, out)) = msg.downcast_ref::<Message>().map(|m| (m.session, m.out.clone())) else {
        return Update::DoNothing;
    };
    let mut save = false;
    let mut again = None;
    let update = {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(note_id) = s
            .voice
            .running
            .as_ref()
            .filter(|r| r.session == session)
            .map(|r| r.note)
        else {
            return Update::DoNothing;
        };
        match out {
            Out::Text(words) => {
                println!("AZREVIEW_DICTATION text {note_id} {words}");
                if let Some(note) = s.notes.iter_mut().find(|n| n.id == note_id) {
                    note.text = dictation::join_text(&note.text, &words);
                    // What was said is kept at once: a window closed mid-dictation loses nothing.
                    save = true;
                }
                Update::RefreshDom
            }
            Out::Failed { status, message } => {
                println!("AZREVIEW_DICTATION refused {status:?} {message}");
                s.problem = Some(refusal(status, &message));
                if status == Status::Unavailable {
                    s.voice.unavailable = Some(message);
                }
                Update::RefreshDom
            }
            Out::Ended(end) => {
                let cause = s
                    .voice
                    .running
                    .as_ref()
                    .and_then(|r| r.stopping)
                    .filter(|_| end == End::Stopped)
                    .unwrap_or_else(|| Cause::from(end));
                println!("AZREVIEW_DICTATION ended {note_id} {}", cause.name());
                s.voice.running = None;
                s.voice.dwell.session_ended();
                // A note that heard nothing is not kept: a marker without words is clutter.
                let kept = s.notes.iter().any(|n| n.id == note_id && !n.text.trim().is_empty());
                if let Some(note) = s.notes.iter_mut().find(|n| n.id == note_id) {
                    note.state = NoteState::Done;
                }
                if !kept {
                    s.notes.retain(|n| n.id != note_id);
                }
                save = kept;
                // A rest reported while this one was finishing starts now, if the pen still rests there.
                again = s.voice.pending.take().filter(|a| s.voice.dwell.still_at(a));
                Update::RefreshDom
            }
        }
    };
    if save {
        crate::save_session(&app, &mut info);
    }
    info.trigger_all_virtual_view_rerender();
    if let Some(anchor) = again {
        return start(&mut app, &mut info, anchor);
    }
    update
}

/// What the user is told when the recogniser refuses: this app's words, then the engine's.
fn refusal(status: Status, message: &str) -> String {
    let key = match status {
        Status::Denied => "azreview-dictation-denied",
        Status::Unavailable => "azreview-dictation-unavailable",
        _ => "azreview-dictation-failed",
    };
    t_args(key, &[("why", Arg::from(message.to_string()))])
}

// ==== What the window asks for ====

/// Stops the running dictation (the stop button).
pub extern "C" fn on_stop(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.voice.request_stop(Cause::Button) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The title row's switch for the side column.
pub extern "C" fn on_toggle_notes(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.voice.notes_open = !s.voice.notes_open;
    Update::RefreshDom
}

/// A note's text was edited (each key).
pub extern "C" fn on_note_text(mut data: RefAny, _info: CallbackInfo, state: TextAreaState) -> OnTextInputReturn {
    let tag = data
        .downcast_ref::<crate::column::NoteTag>()
        .map(|t| (t.app.clone(), t.id));
    if let Some((mut app, id)) = tag {
        let text = state.get_text().as_str().to_string();
        let mut guard = app.downcast_mut::<AppState>();
        if let Some(note) = guard.as_mut().and_then(|s| s.notes.iter_mut().find(|n| n.id == id)) {
            note.text = text;
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// A note was left (the edit is done): the session is kept.
pub extern "C" fn on_note_blur(mut data: RefAny, mut info: CallbackInfo, state: TextAreaState) -> Update {
    let Some((app, id)) = data
        .downcast_ref::<crate::column::NoteTag>()
        .map(|t| (t.app.clone(), t.id))
    else {
        return Update::DoNothing;
    };
    let mut app_ref = app.clone();
    let changed = app_ref.downcast_mut::<AppState>().is_some_and(|mut s| {
        let text = state.get_text().as_str().to_string();
        match s.notes.iter_mut().find(|n| n.id == id) {
            Some(note) if note.text != text => {
                note.text = text;
                true
            }
            _ => false,
        }
    });
    if changed {
        crate::save_session(&app, &mut info);
    }
    Update::DoNothing
}

/// A note's delete button: the note and its marker go; a dictation writing it stops.
pub extern "C" fn on_note_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, id)) = data
        .downcast_ref::<crate::column::NoteTag>()
        .map(|t| (t.app.clone(), t.id))
    else {
        return Update::DoNothing;
    };
    {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        if s.voice.running_note() == Some(id) {
            s.voice.request_stop(Cause::Button);
        }
        s.notes.retain(|n| n.id != id);
    }
    crate::save_session(&app, &mut info);
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// The kit's language as the recogniser's: a BCP 47 tag, or empty for the system's.
pub fn language_of(kit_ref: &RefAny) -> String {
    let mut kit_ref = kit_ref.clone();
    kit_ref
        .downcast_ref::<kit::Kit>()
        .map(|k| k.language().tag().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "voice_test.rs"]
mod voice_test;
