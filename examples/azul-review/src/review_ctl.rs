//! The review session in the window: the always visible key at the top right (Start review / Recording 0:42 / Paused 0:42 and Stop), its two
//! shortcuts, the once-a-second timer that keeps the elapsed time true, and the debug op the scripts start a session with.
//!
//! The policy is `review_session.rs` (pure) and what it asks of the microphone is `Voice::review_event` (voice.rs); this builds the key,
//! answers its clicks and its shortcuts, and refreshes the window. Nothing here opens a microphone by itself, and nothing listens until the
//! person presses Start: a window starts idle.

use azul::{
    callbacks::CallbackType,
    dom::{AccessibilityInfo, AccessibilityRole, VirtualKeyCode},
    error::CustomE2eOpResult,
    json::Json,
    prelude::*,
    str::String as AzString,
    task::TerminateTimer,
    time::SystemTimeDiff,
};
use azul_appkit::{
    l10n::{label, t_args, Arg},
    ui as kit,
};

use crate::{
    dictation::Mic,
    ids,
    review_session::{length_text, Event, State},
    ui, AppState,
};

/// The keys of the shortcuts (Mod is Cmd on a Mac and Ctrl elsewhere).
pub const KEYS_TOGGLE: &str = "Mod+Shift+M";
pub const KEYS_PAUSE: &str = "Mod+Shift+H";

/// How often the elapsed time of a session that records is drawn again, ms.
const TICK_MS: u64 = 1_000;

/// The shortcut as the platform shows it (`⌘⇧M`, `Ctrl+Shift+M`).
#[must_use]
pub fn shown(keys: &str) -> String {
    kit::shown_keys(keys, cfg!(target_os = "macos"))
}

/// The words of the key in each state: what it says on the key, what a screen reader and the tooltip say (with the shortcut).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    pub text: String,
    pub name: String,
}

/// What the main key says in `state` after `elapsed_ms` of the session (English or German by the window's language).
#[must_use]
pub fn words(state: State, elapsed_ms: u64) -> Words {
    let time = length_text(elapsed_ms);
    let args = |keys: &str| [("time", Arg::from(time.clone())), ("keys", Arg::from(shown(keys)))];
    match state {
        State::Idle | State::Stopped => Words {
            text: azul_appkit::l10n::t("azreview-review-start"),
            name: t_args("azreview-review-start-name", &args(KEYS_TOGGLE)),
        },
        State::Recording => Words {
            text: t_args("azreview-review-recording", &args(KEYS_PAUSE)),
            name: t_args("azreview-review-recording-name", &args(KEYS_PAUSE)),
        },
        State::Paused => Words {
            text: t_args("azreview-review-paused", &args(KEYS_PAUSE)),
            name: t_args("azreview-review-paused-name", &args(KEYS_PAUSE)),
        },
    }
}

/// The stop key's name (with its shortcut).
#[must_use]
pub fn stop_name() -> String {
    t_args("azreview-review-stop-name", &[("keys", Arg::from(shown(KEYS_TOGGLE)))])
}

/// Whether the session itself captures audio now: it records AND the microphone is open (kept ready, or taking a dictation).
#[must_use]
pub fn capturing(state: State, mic: Mic) -> bool {
    state == State::Recording && mic.is_open()
}

/// Whether ANY audio is being captured now: the session's microphone (`capturing`) or a voice clip (the Record key, or a press with the audio
/// nib: both are the person's own act and have a level meter and a Stop recording key of their own). The dot is red for this and for nothing else.
#[must_use]
pub fn audio_captured(state: State, mic: Mic, clip_records: bool) -> bool {
    capturing(state, mic) || clip_records
}

/// The dot of the key: red whenever audio is captured, whatever the session says; amber while the session is held and nothing is captured; grey
/// otherwise (a session that records with the microphone closed: it opens when the pen comes near).
#[must_use]
pub fn dot_css(state: State, captured: bool) -> &'static str {
    match (state, captured) {
        (_, true) => "background: system:danger;",
        (State::Paused, false) => "background: system:warning;",
        _ => "background: system:separator;",
    }
}

/// The dot: decorative (the name of the key says the rest).
fn dot(state: State, captured: bool) -> Dom {
    Dom::create_div()
        .with_id(ids::REVIEW_DOT)
        .with_accessibility_info(AccessibilityInfo { role: AccessibilityRole::Nothing, ..AccessibilityInfo::create_default() })
        .with_css(format!("width: 9px; height: 9px; border-radius: 5px; flex-shrink: 0; {}", dot_css(state, captured)).as_str())
}

const PILL: &str = "padding: 0px 8px; gap: 6px; background: system:control-background; border: 1px solid system:separator; \
                    color: system:text; :hover { background: system:selection-background; }";
const START: &str = "padding: 0px 10px; gap: 6px; background: system:accent; border: 1px solid system:accent; color: system:accent-text;";
const STOP: &str = "padding: 0px 6px; gap: 4px; background: system:control-background; border: 1px solid system:separator; \
                    color: system:text; :hover { background: system:selection-background; }";

/// The key of the review session for the top bar: always there. "Start review" while nothing runs; while a session runs the state, the
/// length and the dot (a click pauses, or resumes), and beside it Stop.
#[must_use]
pub fn pill(s: &AppState, data: &RefAny) -> Dom {
    let state = s.voice.review.state();
    let elapsed = s.voice.review.elapsed_ms(s.voice.now_ms());
    let said = words(state, elapsed);
    let name = AzString::from(said.name.as_str());
    let mut group = Dom::create_div().with_id(ids::REVIEW_GROUP).with_css("display: flex; flex-direction: row; align-items: center; gap: 3px;");
    // one red dot while ANY audio is captured: the session's microphone, or a voice clip that records with no session at all
    let captured = audio_captured(state, s.voice.mic, s.recording.is_some());
    match state {
        State::Idle | State::Stopped => {
            let mut start = ui::with_tip(ui::key(START, name.clone(), data.clone(), on_start as CallbackType), &name).with_id(ids::REVIEW_STATE);
            if captured {
                start.add_child(dot(state, true));
            }
            start.add_child(Dom::create_icon("mic").with_css("font-size: 14px;"));
            start.add_child(Dom::create_span_with_text(said.text.as_str()));
            group.add_child(start);
        }
        State::Recording | State::Paused => {
            group.add_child(
                ui::with_tip(ui::key(PILL, name.clone(), data.clone(), on_pause as CallbackType), &name)
                    .with_id(ids::REVIEW_STATE)
                    .with_child(dot(state, captured))
                    .with_child(Dom::create_span_with_text(said.text.as_str())),
            );
            let stop = AzString::from(stop_name().as_str());
            group.add_child(
                ui::with_tip(ui::key(STOP, stop.clone(), data.clone(), on_stop as CallbackType), &stop)
                    .with_id(ids::REVIEW_STOP)
                    .with_child(Dom::create_icon("stop").with_css("font-size: 14px;"))
                    .with_child(Dom::create_span_with_text(label("azreview-review-stop").as_str())),
            );
        }
    }
    group
}

/// What a press (or a shortcut) of an explicit event does: the session takes it, the microphone is opened or closed by the voice, the key's
/// clock is armed, and the window is drawn again.
pub fn apply(data: &mut RefAny, info: &mut CallbackInfo, event: Event) -> Update {
    let capture = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.voice.review_event(event)
    };
    if capture == crate::review_session::Capture::Open {
        // "near" closes the microphone again when the pen has been away: that timer looks after it
        crate::voice::ensure_standby_timer(data, info);
    }
    ensure_timer(data, info);
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

extern "C" fn on_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply(&mut data, &mut info, Event::Start)
}

/// A click on the state: pause what records, resume what is held.
extern "C" fn on_pause(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let event = data.downcast_ref::<AppState>().and_then(|s| s.voice.review.pause_event());
    match event {
        Some(event) => apply(&mut data, &mut info, event),
        None => Update::DoNothing,
    }
}

extern "C" fn on_stop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply(&mut data, &mut info, Event::Stop)
}

/// The shortcuts: Mod+Shift+M starts, or stops what runs; Mod+Shift+H holds (pauses), or resumes. `Some` when the key was one of them.
/// (Not P: Cmd+Shift+P is the recorder window's global pause key on a Mac, and a key that is two things is neither.)
pub fn on_key(data: &mut RefAny, info: &mut CallbackInfo, key: VirtualKeyCode) -> Option<Update> {
    let mods = info.get_key_modifiers();
    if !(mods.primary_down() && mods.shift) {
        return None;
    }
    let event = match key {
        VirtualKeyCode::M => data.downcast_ref::<AppState>().map(|s| s.voice.review.toggle_event()),
        VirtualKeyCode::H => data.downcast_ref::<AppState>().and_then(|s| s.voice.review.pause_event()),
        _ => return None,
    };
    info.prevent_default();
    Some(match event {
        Some(event) => apply(data, info, event),
        // the pause key with nothing to pause is nothing, and it is still this key's
        None => Update::DoNothing,
    })
}

/// The window is asked to close (its close button, Cmd+W, or the close Cmd+Q asks of every window): a session that exists ends and the
/// microphone closes now, before the window goes. The window itself is not held back.
pub extern "C" fn on_close_requested(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.voice.review_event(Event::WindowClose);
    }
    Update::DoNothing
}

/// Arms the timer that draws the elapsed time again every second while a session records (one at a time).
pub fn ensure_timer(data: &mut RefAny, info: &mut CallbackInfo) {
    let wanted = data.downcast_mut::<AppState>().is_some_and(|mut s| {
        let wanted = s.voice.review.listens() && !s.voice.review_ticking;
        if wanted {
            s.voice.review_ticking = true;
        }
        wanted
    });
    if wanted {
        let timer = Timer::create(data.clone(), on_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
        info.add_timer(TimerId::unique(), timer);
    }
}

extern "C" fn on_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let records = data.downcast_mut::<AppState>().is_some_and(|mut s| {
        let records = s.voice.review.listens();
        if !records {
            s.voice.review_ticking = false;
        }
        records
    });
    TimerCallbackReturn {
        should_update: Update::RefreshDom,
        should_terminate: if records { TerminateTimer::Continue } else { TerminateTimer::Terminate },
    }
}

// ==== the debug server's op ====

/// The op that plays an explicit event of the session for the scripts (no key is pressed): the same `apply` the key and the shortcut use.
pub const OP_NAME: &str = "azreview_review";

/// The event an op's arguments stand for.
///
/// # Errors
/// A sentence for the script: arguments that are no JSON, or an action that is none of the five.
pub fn parse_op(args: &str, current: State) -> Result<Event, String> {
    let json: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("the arguments are no JSON: {e}"))?;
    match json.get("action").and_then(serde_json::Value::as_str) {
        Some("start") => Ok(Event::Start),
        Some("pause") => Ok(Event::Pause),
        Some("resume") => Ok(Event::Resume),
        Some("stop") => Ok(Event::Stop),
        Some("toggle") => Ok(match current {
            State::Recording | State::Paused => Event::Stop,
            State::Idle | State::Stopped => Event::Start,
        }),
        other => Err(format!("\"action\" is one of start, pause, resume, stop, toggle, not {other:?}")),
    }
}

/// Adds the op to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"start, pause, resume or stop the review session (what the key at the top right does)\",\
         \"description\":\"action: start, pause, resume, stop or toggle. Answers the session's state. Nothing listens before a start.\",\
         \"args\":[{{\"name\":\"action\",\"type\":\"string\"}}],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str())).into_result().unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
}

fn answer(json: &str) -> CustomE2eOpResult {
    CustomE2eOpResult { handled: true, json: AzString::from(json) }
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_NAME {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let current = data.downcast_ref::<AppState>().map_or(State::Idle, |s| s.voice.review.state());
    let event = match parse_op(args.as_str(), current) {
        Ok(event) => event,
        Err(why) => return answer(&serde_json::json!({"success": false, "error": why}).to_string()),
    };
    let _ = apply(&mut data, &mut info, event);
    let state = data.downcast_ref::<AppState>().map_or(State::Idle, |s| s.voice.review.state());
    answer(&serde_json::json!({"success": true, "state": state.name()}).to_string())
}

#[cfg(test)]
#[path = "review_ctl_test.rs"]
mod review_ctl_test;
