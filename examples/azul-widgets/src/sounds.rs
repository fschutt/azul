//! The "Sounds" section: the sound board. One button per system sound and one per haptic pattern,
//! each saying which route it takes on THIS device and which OS switch applied.
//!
//! A button of the first group asks for the sound with `CallbackInfo::play_system_sound` and prints
//! `CallbackInfo::get_sound_route`: `system:android:FX_KEY_CLICK` (the click of "Touch sounds"),
//! `ringtone:android:notification` (the user's own tone), `media-action:android:SHUTTER_CLICK`,
//! `haptic:Selection` (iOS answers with a tap), or `none:<why>` (`none:no-system-route`,
//! `none:os:input-feedback-sounds-off`, `none:app-muted`). The window's recording, one line per
//! request with what became of it (`played`, `skipped:os:touch-sounds-off`, ...), is printed under
//! the buttons; it shows the outcome of the EARLIER presses (a sound is decided at the end of the
//! pass that asked for it).
//!
//! A button of the second group plays the haptic pattern on the phone's own actuator.
//!
//! To find out which sound is which on a phone: press the buttons one at a time and listen. The
//! sounds of the first group that go through `AudioManager` are the ones the system itself plays;
//! turning Settings > Sound & vibration > Touch sounds off silences exactly those, and the board
//! says so (`skipped:os:touch-sounds-off`).

use azul::{
    audio::SystemSound,
    gesture::{HapticPattern, HapticTarget},
    prelude::*,
    widgets::*,
};

use crate::{captioned, section, Showcase};

/// Every system sound, in the order of the enum, with the word on its button.
static SOUNDS: [(SystemSound, &str); 25] = [
    (SystemSound::Click, "Click"),
    (SystemSound::Tick, "Tick"),
    (SystemSound::ToggleOn, "ToggleOn"),
    (SystemSound::ToggleOff, "ToggleOff"),
    (SystemSound::MenuOpen, "MenuOpen"),
    (SystemSound::MenuClose, "MenuClose"),
    (SystemSound::Expand, "Expand"),
    (SystemSound::Collapse, "Collapse"),
    (SystemSound::TabChange, "TabChange"),
    (SystemSound::Back, "Back"),
    (SystemSound::DragStart, "DragStart"),
    (SystemSound::DropAccept, "DropAccept"),
    (SystemSound::DropReject, "DropReject"),
    (SystemSound::Notification, "Notification"),
    (SystemSound::Success, "Success"),
    (SystemSound::Info, "Info"),
    (SystemSound::Question, "Question"),
    (SystemSound::Warning, "Warning"),
    (SystemSound::Error, "Error"),
    (SystemSound::Bell, "Bell"),
    (SystemSound::Delete, "Delete"),
    (SystemSound::TrashEmpty, "TrashEmpty"),
    (SystemSound::Send, "Send"),
    (SystemSound::CameraShutter, "CameraShutter"),
    (SystemSound::ScreenCapture, "ScreenCapture"),
];

/// Every haptic pattern, in the order of the enum, with the word on its button.
static HAPTICS: [(HapticPattern, &str); 19] = [
    (HapticPattern::Selection, "Selection"),
    (HapticPattern::ImpactLight, "ImpactLight"),
    (HapticPattern::ImpactMedium, "ImpactMedium"),
    (HapticPattern::ImpactHeavy, "ImpactHeavy"),
    (HapticPattern::ImpactSoft, "ImpactSoft"),
    (HapticPattern::ImpactRigid, "ImpactRigid"),
    (HapticPattern::Success, "Success"),
    (HapticPattern::Warning, "Warning"),
    (HapticPattern::Error, "Error"),
    (HapticPattern::KeyPress, "KeyPress"),
    (HapticPattern::KeyRelease, "KeyRelease"),
    (HapticPattern::LongPress, "LongPress"),
    (HapticPattern::ContextClick, "ContextClick"),
    (HapticPattern::TextHandleMove, "TextHandleMove"),
    (HapticPattern::GestureStart, "GestureStart"),
    (HapticPattern::GestureEnd, "GestureEnd"),
    (HapticPattern::Rise, "Rise"),
    (HapticPattern::Fall, "Fall"),
    (HapticPattern::Spin, "Spin"),
];

/// The most lines of the recording the board prints.
const LOG_LINES: usize = 8;

/// What the section shows. Lives in `Showcase::sounds`.
#[derive(Debug, Clone, Default)]
pub(crate) struct SoundsDemo {
    /// What the last button did: its name and its route.
    last: String,
    /// The window's recording of the last requests, oldest first.
    log: Vec<String>,
}

/// The data of one sound button: the page's own data and the sound it asks for.
struct SoundPress {
    app: RefAny,
    sound: SystemSound,
    name: &'static str,
}

/// The data of one haptic button.
struct HapticPress {
    app: RefAny,
    pattern: HapticPattern,
    name: &'static str,
}

extern "C" fn on_sound(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut app, sound, name) = match data.downcast_ref::<SoundPress>() {
        Some(press) => (press.app.clone(), press.sound, press.name),
        None => return Update::DoNothing,
    };
    // the route is named before the sound is asked for: it is a pure function of the OS's switches
    let route = info.get_sound_route(sound);
    info.play_system_sound(sound);
    let log: Vec<String> = info
        .get_sound_log()
        .as_ref()
        .iter()
        .map(|line| line.as_str().to_string())
        .collect();
    match app.downcast_mut::<Showcase>() {
        Some(mut showcase) => {
            showcase.sounds.last = format!("{name}: {}", route.as_str());
            showcase.sounds.log = log;
            showcase.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_haptic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut app, pattern, name) = match data.downcast_ref::<HapticPress>() {
        Some(press) => (press.app.clone(), press.pattern, press.name),
        None => return Update::DoNothing,
    };
    info.play_haptic(pattern, HapticTarget::System);
    match app.downcast_mut::<Showcase>() {
        Some(mut showcase) => {
            showcase.sounds.last = format!("haptic {name}: the actuator of this device");
            showcase.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// What the board knows of the device before a button is pressed.
fn device_line(info: &LayoutCallbackInfo) -> String {
    let platform = if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(target_os = "ios") {
        "iOS"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    };
    let audio = info.get_system_style().audio;
    let touch = if audio.input_feedback_sounds_enabled {
        "on"
    } else {
        "off (Settings > Sound & vibration > Touch sounds)"
    };
    let events = if audio.event_sounds_enabled { "on" } else { "off" };
    format!("{platform}. Input feedback sounds (Touch sounds): {touch}. Event sounds: {events}.")
}

fn note(text: &str) -> Dom {
    Dom::create_span_with_text(text).with_css("font-size: 12px; color: system:secondary-text;")
}

/// The section's card. `theme` is the page's widget theme.
pub(crate) fn sounds_section(
    data: &RefAny,
    state: &SoundsDemo,
    info: &LayoutCallbackInfo,
    theme: UiTheme,
) -> Dom {
    let row = "display: flex; flex-direction: row; flex-wrap: wrap; gap: 8px;";

    let mut sounds = Dom::create_div().with_css(row);
    for (sound, name) in &SOUNDS {
        sounds.add_child(
            Button::create(*name)
                .with_on_click(
                    RefAny::new(SoundPress {
                        app: data.clone(),
                        sound: *sound,
                        name: *name,
                    }),
                    on_sound,
                )
                .with_theme(theme)
                .dom(),
        );
    }

    let mut haptics = Dom::create_div().with_css(row);
    for (pattern, name) in &HAPTICS {
        // "Haptic Error" and not "Error": the sound of the same name is a button of its own
        haptics.add_child(
            Button::create(format!("Haptic {name}"))
                .with_on_click(
                    RefAny::new(HapticPress {
                        app: data.clone(),
                        pattern: *pattern,
                        name: *name,
                    }),
                    on_haptic,
                )
                .with_theme(theme)
                .dom(),
        );
    }

    let last = if state.last.is_empty() {
        "Press a button."
    } else {
        state.last.as_str()
    };
    let mut log = Dom::create_div().with_css("display: flex; flex-direction: column;");
    if state.log.is_empty() {
        log.add_child(note("Nothing asked for yet."));
    }
    for line in state.log.iter().rev().take(LOG_LINES).rev() {
        log.add_child(note(line));
    }

    section(
        "Sounds",
        vec![
            captioned("This device", note(&device_line(info))),
            captioned(
                "System sounds: one per meaning, the route it takes on this device",
                sounds,
            ),
            captioned(
                "Haptic patterns: the actuator of this device",
                haptics,
            ),
            captioned(
                "Last button",
                Dom::create_span_with_text(last).with_css("color: system:text;"),
            ),
            captioned(
                "What the window recorded of the earlier requests (route, why it did not play, outcome)",
                log,
            ),
        ],
    )
}
