//! The azul side of the screens: the callbacks (one `on_act` for every tap, the pad's pointer and pinch events, the
//! editor's text fields), the layout function, the timer that lets the connection work, and the window. Each callback
//! only turns what the engine says into an [`Action`] or a [`PadEvent`] and hands it to [`Root`]; what they do is in
//! `ui/mod.rs` and tested there.

use azul::{
    callbacks::{TextAreaOnTextInputCallbackType, TextInputOnTextInputCallbackType},
    dom::DomNodeId,
    gesture::{HapticPattern, HapticTarget},
    option::OptionPixelValue,
    prelude::*,
    str::String as AzString,
    widgets::{OnTextInputReturn, TextAreaState, TextInputState, TextInputValid},
};
use azul_review_link::frame::Haptic;
use azul_review_link::limits::{clean_text, MAX_BUTTON_TEXT_CHARS, MAX_LABEL_CHARS};

use super::kit::Cx;
use super::{chrome, Action, EnvSource, PadEvent, Root};
use azul::time::SystemTimeDiff;
use crate::env::{Env, Insets, Lang, Platform};
use crate::layout;

/// What a tap carries: the app and what to do.
pub struct ActCtx {
    pub root: RefAny,
    pub action: Action,
}

/// The two fields of a quick button in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Label,
    Instruction,
}

/// What a text field of the editor carries.
pub struct FieldCtx {
    pub root: RefAny,
    pub slot: usize,
    pub field: Field,
}

/// A tap (a click, a press of Enter or Space on a focused key): does what the key says.
pub extern "C" fn on_act(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut root_ref, action)) = data.downcast_ref::<ActCtx>().map(|c| (c.root.clone(), c.action.clone())) else {
        return Update::DoNothing;
    };
    let Some(mut root) = root_ref.downcast_mut::<Root>() else {
        return Update::DoNothing;
    };
    let now = root.now();
    if root.perform(now, action) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// A callback that carries the app and an action, for a node that is no key (a long press on one).
#[must_use]
pub fn with_action(dom: Dom, filter: HoverEventFilter, root: &RefAny, action: Action) -> Dom {
    dom.with_callback(EventFilter::Hover(filter), RefAny::new(ActCtx { root: root.clone(), action }), on_act)
}

// ---- the pad

/// Where the pointer is on the pad (the pad's own layout pixels) and how big the pad is. The engine answers the cursor local
/// to the node the callback sits on, and undoes a transform that paints the screen smaller (the simulator's window on a small
/// screen), so a drag of a given length on the pad is the same drag at every scale.
fn pad_point(info: &CallbackInfo) -> Option<(f32, f32, (f32, f32))> {
    let local = info.get_cursor_relative_to_node().into_option()?;
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, AzString::from(super::remote::PAD_ID));
    if node.into_raw() == 0 {
        return None;
    }
    let rect = info.get_node_rect(DomNodeId { dom, node }).into_option()?;
    Some((local.x, local.y, (rect.size.width, rect.size.height)))
}

fn pad_event(mut data: RefAny, mut info: CallbackInfo, make: impl FnOnce(f32, f32) -> PadEvent) -> Update {
    let Some(mut root) = data.downcast_mut::<Root>() else {
        return Update::DoNothing;
    };
    let Some((x, y, pad)) = pad_point(&info) else {
        return Update::DoNothing;
    };
    let now = root.now();
    let before = root.ui.pad.live().is_some();
    let changed = root.pad_event(now, make(x, y), pad);
    if root.ui.pad.live().is_some() || before {
        info.prevent_default();
    }
    if changed {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

pub extern "C" fn on_pad_down(data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.capture_pointer(node);
    pad_event(data, info, |x, y| PadEvent::Down { x, y })
}

pub extern "C" fn on_pad_move(data: RefAny, info: CallbackInfo) -> Update {
    pad_event(data, info, |x, y| PadEvent::Move { x, y })
}

pub extern "C" fn on_pad_up(data: RefAny, mut info: CallbackInfo) -> Update {
    info.release_pointer_capture();
    pad_event(data, info, |x, y| PadEvent::Up { x, y })
}

pub extern "C" fn on_pad_secondary(data: RefAny, info: CallbackInfo) -> Update {
    pad_event(data, info, |_, _| PadEvent::Secondary)
}

/// A pinch (a trackpad pinch in the simulator, two fingers on a phone): zooms around the pen.
pub extern "C" fn on_pad_pinch(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pinch) = info.get_pinch().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut root) = data.downcast_mut::<Root>() else {
        return Update::DoNothing;
    };
    let now = root.now();
    info.prevent_default();
    if root.pad_event(now, PadEvent::Pinch { scale: pinch.scale, began: pinch.began, ended: pinch.ended }, (0.0, 0.0)) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

// ---- the editor's fields

fn field_input(mut data: RefAny, typed: String) -> OnTextInputReturn {
    let Some((mut root_ref, slot, field)) = data.downcast_ref::<FieldCtx>().map(|c| (c.root.clone(), c.slot, c.field)) else {
        return OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes };
    };
    let Some(mut root) = root_ref.downcast_mut::<Root>() else {
        return OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes };
    };
    let (max, keep_newlines, what) = match field {
        Field::Label => (MAX_LABEL_CHARS, false, "button label"),
        Field::Instruction => (MAX_BUTTON_TEXT_CHARS, true, "button text"),
    };
    let (kept, cut) = clean_text(&typed, max, keep_newlines, what);
    if slot >= root.ui.drafts.len() {
        return OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::No };
    }
    match field {
        Field::Label => {
            root.ui.drafts[slot].0 = kept;
            root.ui.draft_cut[slot].0 = cut.clone();
        }
        Field::Instruction => {
            root.ui.drafts[slot].1 = kept;
            root.ui.draft_cut[slot].1 = cut.clone();
        }
    }
    root.ui.editor_dirty = true;
    // a longer text is refused as it is typed: the field keeps what fitted
    OnTextInputReturn {
        update: Update::RefreshDom,
        valid: if cut.is_some() { TextInputValid::No } else { TextInputValid::Yes },
    }
}

/// The label of a quick button was typed in.
pub extern "C" fn on_label_input(data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    field_input(data, state.get_text().as_str().to_string())
}

/// The instruction of a quick button was typed in.
pub extern "C" fn on_text_input(data: RefAny, _info: CallbackInfo, state: TextAreaState) -> OnTextInputReturn {
    field_input(data, state.get_text().as_str().to_string())
}

/// The callback type the text field takes.
pub const LABEL_CALLBACK: TextInputOnTextInputCallbackType = on_label_input;
/// The callback type the text field takes.
pub const TEXT_CALLBACK: TextAreaOnTextInputCallbackType = on_text_input;

// ---- the window

/// What the window says the phone is like.
fn window_env(info: &LayoutCallbackInfo) -> Env {
    let (width, height) = (info.get_window_width(), info.get_window_height());
    let safe = info.get_safe_area_insets();
    let px = |v: OptionPixelValue| v.into_option().map_or(0.0, |p| p.to_pixels(0.0, 16.0, 16.0));
    let insets = Insets { top: px(safe.top), right: px(safe.right), bottom: px(safe.bottom), left: px(safe.left) }.held(width, height);
    let platform = if cfg!(target_os = "android") { Platform::Android } else { Platform::Ios };
    Env { width, height, insets, platform, lang: Lang::from_tag(info.get_locale().as_str()) }
}

/// The layout callback: the screens as the state says (the simulator puts them in a phone beside its panel).
pub extern "C" fn layout(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let mut shared = data.clone();
    let Some(root) = shared.downcast_ref::<Root>() else {
        return Dom::create_body();
    };
    let env = match root.env {
        EnvSource::Fixed(env) => env,
        EnvSource::Window => window_env(&info),
    };
    let ribbon = root.api.view().pen.as_ref().is_some_and(|p| p.return_point.is_some());
    let cx = Cx { root: &root, env: &env, data: &data, z: layout::zones_with(&env, ribbon) };
    let phone = chrome::phone(&cx);
    match &root.sim {
        Some(sim) => crate::sim::wrap(&cx, sim, phone),
        None => phone,
    }
}

/// The timer that lets the connection work: about 30 times a second it receives, sends and times out, and rebuilds the
/// screens when something they show changed.
pub extern "C" fn tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut root) = data.downcast_mut::<Root>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let now = root.now();
    let mut changed = root.tick(now);
    if let Some(buzz) = root.take_haptic() {
        let pattern = match buzz {
            Haptic::Success => HapticPattern::Success,
            Haptic::Warning => HapticPattern::Warning,
            Haptic::Error => HapticPattern::Error,
        };
        info.callback_info.play_haptic(pattern, HapticTarget::System);
    }
    if let Some(mut sim) = root.sim.take() {
        changed |= sim.tick(&mut root, now, &mut info.callback_info);
        root.sim = Some(sim);
    }
    if changed {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// How often the timer runs.
pub const TICK_MS: u64 = 33;

/// The window is created: the timer starts.
pub extern "C" fn on_window_created(data: RefAny, mut info: CallbackInfo) -> Update {
    let timer = Timer::create(data.clone(), tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
    info.add_timer(TimerId::unique(), timer);
    Update::DoNothing
}

/// The window of the phone's own app: the title and the timer on creation; the size is the screen's.
#[must_use]
pub fn phone_window_options(title: &str) -> WindowCreateOptions {
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = AzString::from(title);
    window.create_callback = Some(Callback::create(on_window_created)).into();
    window
}

/// The window of the app: the title, the size (a phone's own, or the simulator's), the timer on creation.
#[must_use]
pub fn window_options(title: &str, size: (f32, f32), dpi: u32) -> WindowCreateOptions {
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = AzString::from(title);
    window.window_state.size.dimensions = LogicalSize::create(size.0, size.1);
    window.window_state.size.dpi = dpi;
    window.create_callback = Some(Callback::create(on_window_created)).into();
    window
}
