//! System sounds: the short sounds a platform plays for its own user interface, asked for by MEANING.
//!
//! The app says WHAT happened ("the tab changed", "an error", "a notification arrived") with a
//! [`SystemSound`]; the platform backend plays what the OS itself plays for that. Nothing is
//! bundled: a sound the platform has no system route for answers [`SoundRoute::Silent`] with the
//! reason `no-system-route`, which the recording and the log say out loud, never silently.
//!
//! It is the output twin of [`crate::haptics`] and follows its path exactly: `CallbackInfo`
//! queues a request, the shell drains the queue once per outermost event pass, and the backend
//! plays. What the haptics path does not have, and a sound needs, is a DECISION between the
//! request and the OS call: which route the platform has for the sound, whether the app or an OS
//! switch silences it, and whether a fling of events would stack the same sound. That decision is
//! [`route`] (a pure function of the sound, the platform, the app's config and the OS's
//! [`AudioMetrics`]) and [`SoundManager::process`] (the pacing), so every host can test them.
//!
//! # Two classes of sound
//!
//! | class | sounds | OS switch that silences it |
//! |---|---|---|
//! | [`SoundClass::InputFeedback`] | `Click`, `Tick`, `ToggleOn`, `TabChange`, `Back`, ... | Android "Touch sounds", GTK `input-feedback-sounds` |
//! | [`SoundClass::Event`] | `Notification`, `Success`, `Error`, `CameraShutter`, ... | the ringer mode and Do Not Disturb, GTK `event-sounds` |
//!
//! # Platform routes (system routes only: no audio file is bundled)
//!
//! | sound | Android | iOS |
//! |---|---|---|
//! | `Click`, `ToggleOn`, `ToggleOff`, `TabChange`, `DropAccept` | `AudioManager.FX_KEY_CLICK` | selection haptic for the toggles and the tab change, none for the click |
//! | `Tick` | `AudioManager.FX_FOCUS_NAVIGATION_DOWN` | selection haptic |
//! | `Back` | `AudioManager.FX_BACK` (API 31, else `FX_KEY_CLICK`) | none |
//! | `Notification`, `Success`, `Info`, `Question` | the user's notification sound, through `RingtoneManager` | success haptic for `Success` only |
//! | `Warning`, `Error`, `Bell`, `DropReject` | `AudioManager.FX_KEYPRESS_INVALID` | warning or error haptic |
//! | `CameraShutter`, `ScreenCapture` | `MediaActionSound.SHUTTER_CLICK` | none |
//! | the rest | none | none |
//!
//! Desktop platforms answer `none:no-system-route` until their routes land.

use alloc::{collections::VecDeque, format, string::String, vec::Vec};

use azul_css::system::AudioMetrics;

use crate::haptics::HapticPattern;

/// How many sounds there are: the length of [`ALL_SYSTEM_SOUNDS`].
pub const SYSTEM_SOUND_COUNT: usize = 25;

/// Every [`SystemSound`], in declaration order.
pub static ALL_SYSTEM_SOUNDS: [SystemSound; SYSTEM_SOUND_COUNT] = [
    SystemSound::Click,
    SystemSound::Tick,
    SystemSound::ToggleOn,
    SystemSound::ToggleOff,
    SystemSound::MenuOpen,
    SystemSound::MenuClose,
    SystemSound::Expand,
    SystemSound::Collapse,
    SystemSound::TabChange,
    SystemSound::Back,
    SystemSound::DragStart,
    SystemSound::DropAccept,
    SystemSound::DropReject,
    SystemSound::Notification,
    SystemSound::Success,
    SystemSound::Info,
    SystemSound::Question,
    SystemSound::Warning,
    SystemSound::Error,
    SystemSound::Bell,
    SystemSound::Delete,
    SystemSound::TrashEmpty,
    SystemSound::Send,
    SystemSound::CameraShutter,
    SystemSound::ScreenCapture,
];

/// A system sound, named by what it MEANS and not by the file or the id that plays it.
///
/// The set is the union of what the platforms' own toolkits have a sound for. A platform that has
/// none for a meaning walks [`SystemSound::fallback`] and, when nothing on the way has a route,
/// plays nothing and says so ([`Silence::NoSystemRoute`]).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SystemSound {
    // --- input feedback: silenced by the OS's "input feedback" switch where there is one ---
    /// A control was pressed: a button, a row, a key.
    Click,
    /// A discrete step of a continuous movement: a list item passing, a slider detent. The
    /// lightest sound there is; safe to ask for while a finger drags.
    Tick,
    /// A switch, a check box or a radio button turned on.
    ToggleOn,
    /// A switch, a check box or a radio button turned off.
    ToggleOff,
    /// A menu, a popover or a drop-down opened.
    MenuOpen,
    /// A menu, a popover or a drop-down closed.
    MenuClose,
    /// A section, a tree node or an accordion opened.
    Expand,
    /// A section, a tree node or an accordion closed.
    Collapse,
    /// A tab or a page of a pager became the current one. Asked for once per page crossed,
    /// also during a fling, so [`SystemSound::min_interval_ms`] is the short one.
    TabChange,
    /// The user went one step back.
    Back,
    /// A drag began.
    DragStart,
    /// A drag ended on a target that takes it.
    DropAccept,
    /// A drag ended on a target that refuses it.
    DropReject,
    // --- events and actions: silenced by the OS's "event sounds" switch ---
    /// Something arrived that wants the user: a message, a reminder.
    Notification,
    /// A task finished successfully.
    Success,
    /// Something the user should know, which asks nothing of them.
    Info,
    /// The app asks the user a question.
    Question,
    /// A task finished, but something needs attention.
    Warning,
    /// A task failed or an action was refused.
    Error,
    /// A plain attention beep, the terminal bell.
    Bell,
    /// Something was deleted.
    Delete,
    /// The trash was emptied.
    TrashEmpty,
    /// A message was sent.
    Send,
    /// A photo was taken. Some regions require this sound by law, so on Android the app's mute
    /// does not waive it when the OS says it must play.
    CameraShutter,
    /// The screen was captured.
    ScreenCapture,
}

/// Which OS switch silences a [`SystemSound`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SoundClass {
    /// Feedback to the user's own input: clicks, ticks, toggles. The OS's "touch sounds" or
    /// "input feedback sounds" switch silences it.
    InputFeedback,
    /// Something happened: a notification, a result, a shutter. The ringer mode, Do Not Disturb
    /// and the OS's "event sounds" switch silence it.
    Event,
}

impl SoundClass {
    /// The word the recording and the E2E ops use: `input` or `event`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::InputFeedback => "input",
            Self::Event => "event",
        }
    }
}

impl SystemSound {
    /// Which OS switch silences this sound.
    #[must_use]
    pub const fn class(self) -> SoundClass {
        match self {
            Self::Click
            | Self::Tick
            | Self::ToggleOn
            | Self::ToggleOff
            | Self::MenuOpen
            | Self::MenuClose
            | Self::Expand
            | Self::Collapse
            | Self::TabChange
            | Self::Back
            | Self::DragStart
            | Self::DropAccept
            | Self::DropReject => SoundClass::InputFeedback,
            Self::Notification
            | Self::Success
            | Self::Info
            | Self::Question
            | Self::Warning
            | Self::Error
            | Self::Bell
            | Self::Delete
            | Self::TrashEmpty
            | Self::Send
            | Self::CameraShutter
            | Self::ScreenCapture => SoundClass::Event,
        }
    }

    /// The variant's name: what the recording prints and `assert_sound` takes in `sound`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::Tick => "Tick",
            Self::ToggleOn => "ToggleOn",
            Self::ToggleOff => "ToggleOff",
            Self::MenuOpen => "MenuOpen",
            Self::MenuClose => "MenuClose",
            Self::Expand => "Expand",
            Self::Collapse => "Collapse",
            Self::TabChange => "TabChange",
            Self::Back => "Back",
            Self::DragStart => "DragStart",
            Self::DropAccept => "DropAccept",
            Self::DropReject => "DropReject",
            Self::Notification => "Notification",
            Self::Success => "Success",
            Self::Info => "Info",
            Self::Question => "Question",
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Bell => "Bell",
            Self::Delete => "Delete",
            Self::TrashEmpty => "TrashEmpty",
            Self::Send => "Send",
            Self::CameraShutter => "CameraShutter",
            Self::ScreenCapture => "ScreenCapture",
        }
    }

    /// The sound with this name, ignoring case and surrounding spaces. `None` for a name that is
    /// not one of [`ALL_SYSTEM_SOUNDS`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let wanted = name.trim();
        ALL_SYSTEM_SOUNDS
            .iter()
            .copied()
            .find(|sound| sound.name().eq_ignore_ascii_case(wanted))
    }

    /// The position of this sound in [`ALL_SYSTEM_SOUNDS`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The next-simplest sound to try when the platform has no route for this one.
    ///
    /// The ladder follows MEANING, like [`HapticPattern::fallback`]: a drop that was refused
    /// sounds like an error, a trash that was emptied like a delete. Every chain ends at a sound
    /// whose fallback is `None`, and no chain loops.
    #[must_use]
    pub const fn fallback(self) -> Option<Self> {
        match self {
            Self::Expand => Some(Self::MenuOpen),
            Self::Collapse => Some(Self::MenuClose),
            Self::TabChange => Some(Self::Tick),
            Self::Tick | Self::ToggleOn | Self::ToggleOff | Self::Back | Self::DropAccept => {
                Some(Self::Click)
            }
            Self::DropReject => Some(Self::Error),
            Self::Question => Some(Self::Info),
            Self::TrashEmpty => Some(Self::Delete),
            Self::ScreenCapture => Some(Self::CameraShutter),
            Self::Click
            | Self::MenuOpen
            | Self::MenuClose
            | Self::DragStart
            | Self::Notification
            | Self::Success
            | Self::Info
            | Self::Warning
            | Self::Error
            | Self::Bell
            | Self::Delete
            | Self::Send
            | Self::CameraShutter => None,
        }
    }

    /// The haptic pattern that carries this meaning where the platform's idiom is a tap and not
    /// a sound: the toggles, the tick and the tab change are a selection, and the three results
    /// are the notification haptics. `None` for a meaning that has no haptic idiom.
    #[must_use]
    pub const fn haptic(self) -> Option<HapticPattern> {
        match self {
            Self::Tick | Self::ToggleOn | Self::ToggleOff | Self::TabChange => {
                Some(HapticPattern::Selection)
            }
            Self::Success => Some(HapticPattern::Success),
            Self::Warning => Some(HapticPattern::Warning),
            Self::Error | Self::DropReject => Some(HapticPattern::Error),
            Self::Click
            | Self::MenuOpen
            | Self::MenuClose
            | Self::Expand
            | Self::Collapse
            | Self::Back
            | Self::DragStart
            | Self::DropAccept
            | Self::Notification
            | Self::Info
            | Self::Question
            | Self::Bell
            | Self::Delete
            | Self::TrashEmpty
            | Self::Send
            | Self::CameraShutter
            | Self::ScreenCapture => None,
        }
    }

    /// The shortest time between two plays of this sound, in milliseconds.
    ///
    /// A sound asked for faster than this is dropped (`min-interval`): a drag that asks for a
    /// tick per frame must not play a buzz. The tick and the tab change are the short ones, so a
    /// fling through pages still ticks once per page.
    #[must_use]
    pub const fn min_interval_ms(self) -> u64 {
        match self {
            Self::Tick | Self::TabChange => 40,
            Self::Click
            | Self::ToggleOn
            | Self::ToggleOff
            | Self::MenuOpen
            | Self::MenuClose
            | Self::Expand
            | Self::Collapse
            | Self::Back
            | Self::DragStart
            | Self::DropAccept
            | Self::DropReject
            | Self::Notification
            | Self::Success
            | Self::Info
            | Self::Question
            | Self::Warning
            | Self::Error
            | Self::Bell
            | Self::Delete
            | Self::TrashEmpty
            | Self::Send
            | Self::CameraShutter
            | Self::ScreenCapture => 100,
        }
    }
}

/// Whether widgets make sounds of their own accord (what a button does when it is clicked).
///
/// It does not affect a sound the app asks for with `play_system_sound`: an explicit request is
/// the app's own decision.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum WidgetSounds {
    /// The platform's own toolkit's habit: Android clicks when "Touch sounds" is on; iOS, macOS,
    /// Windows and KDE are silent.
    #[default]
    Native,
    /// Widgets always ask for their sounds; the OS switches still apply.
    On,
    /// Widgets never make a sound.
    Off,
}

/// What the app decided about sounds.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SoundConfig {
    /// `false` mutes every sound the app asks for (a kiosk, a test, a recording). The OS is in
    /// charge by default: Microsoft's guide says an app should not carry a mute control of its
    /// own, so this is not a setting to put in front of the user.
    pub enabled: bool,
    /// What the widgets do without being asked.
    pub widget_sounds: WidgetSounds,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            widget_sounds: WidgetSounds::Native,
        }
    }
}

impl SoundConfig {
    /// Whether a widget asks for its own sound on this platform.
    #[must_use]
    pub const fn widgets_play(self, platform: SoundPlatform) -> bool {
        if !self.enabled {
            return false;
        }
        match self.widget_sounds {
            WidgetSounds::On => true,
            WidgetSounds::Off => false,
            WidgetSounds::Native => matches!(platform, SoundPlatform::Android),
        }
    }
}

/// The operating systems a route table exists for.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SoundPlatform {
    /// Android.
    Android,
    /// iOS and iPadOS.
    Ios,
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
    /// Linux and the other desktops that follow the freedesktop sound specification.
    Linux,
    /// Anything else: the web, a build without an operating system.
    Other,
}

impl SoundPlatform {
    /// The platform this build runs on.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "android") {
            Self::Android
        } else if cfg!(target_os = "ios") {
            Self::Ios
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }

    /// The word the route labels use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Other => "other",
        }
    }
}

/// Why nothing plays.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Silence {
    /// The app muted its sounds ([`SoundConfig::enabled`] is `false`).
    AppMuted,
    /// The OS's switch for input feedback is off (Android "Touch sounds").
    OsInputFeedbackOff,
    /// The OS's switch for event sounds is off.
    OsEventSoundsOff,
    /// The platform has no system sound for this meaning, and no sound is bundled.
    NoSystemRoute,
}

impl Silence {
    /// The text the recording, the log and `assert_sound`'s `suppressed_by` use.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::AppMuted => "app-muted",
            Self::OsInputFeedbackOff => "os:input-feedback-sounds-off",
            Self::OsEventSoundsOff => "os:event-sounds-off",
            Self::NoSystemRoute => "no-system-route",
        }
    }
}

/// The request was the same as the one before it in the same pass and was merged into it.
pub const COALESCED: &str = "coalesced";
/// The same sound played less than [`SystemSound::min_interval_ms`] ago.
pub const MIN_INTERVAL: &str = "min-interval";
/// An event sound would be the third identical one in a row.
pub const REPEAT_LIMIT: &str = "repeat-limit";

/// An event sound is not repeated more than this often in a row.
///
/// Microsoft's guide says "don't repeat a sound more than two times consecutively". Input
/// feedback is exempt: a tick per page of a fling is the point.
pub const MAX_IDENTICAL_IN_A_ROW: u8 = 2;
/// Two identical event sounds belong to one run when less than this apart, in milliseconds.
pub const RUN_GAP_MS: u64 = 1000;
/// At most this many requests wait for the next drain; the rest are dropped and counted.
pub const MAX_PENDING_SOUNDS: usize = 256;
/// The recording keeps the last this many requests.
pub const MAX_SOUND_RECORDS: usize = 1024;

/// What the platform does for one sound: the decision [`route`] makes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SoundRoute {
    /// Nothing plays, for this reason.
    Silent(Silence),
    /// A sound effect of the platform's UI sound set, played by name: Android
    /// `AudioManager.playSoundEffect` with the `FX_*` constant `name`, or `fallback` where the
    /// constant does not exist on this OS version.
    Effect {
        /// The platform.
        platform: SoundPlatform,
        /// The constant's name, e.g. `FX_KEY_CLICK`.
        name: &'static str,
        /// The constant to use when `name` does not exist on this OS version.
        fallback: Option<&'static str>,
    },
    /// The user's chosen tone of this kind, played through the platform's ringtone service with
    /// the usage that makes the ringer mode and Do Not Disturb apply (Android `RingtoneManager`).
    Ringtone {
        /// The platform.
        platform: SoundPlatform,
        /// The kind of tone: `notification`.
        kind: &'static str,
    },
    /// A sound of the platform's camera sound set (Android `MediaActionSound`).
    MediaAction {
        /// The platform.
        platform: SoundPlatform,
        /// The constant's name, e.g. `SHUTTER_CLICK`.
        name: &'static str,
        /// `true` when the app is muted: the sound plays only where the OS says it must (a
        /// region that requires a shutter sound).
        only_if_required: bool,
    },
    /// The platform's idiom for this meaning is a tap, not a sound (iOS).
    Haptic(HapticPattern),
}

impl SoundRoute {
    /// The text of the route, as the recording, the log and `assert_sound`'s `route` have it:
    /// `system:android:FX_KEY_CLICK`, `ringtone:android:notification`,
    /// `media-action:android:SHUTTER_CLICK`, `haptic:Selection`, `none:no-system-route`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Silent(why) => {
                let code = why.code();
                format!("none:{code}")
            }
            Self::Effect { platform, name, .. } => {
                let platform = platform.name();
                format!("system:{platform}:{name}")
            }
            Self::Ringtone { platform, kind } => {
                let platform = platform.name();
                format!("ringtone:{platform}:{kind}")
            }
            Self::MediaAction { platform, name, .. } => {
                let platform = platform.name();
                format!("media-action:{platform}:{name}")
            }
            Self::Haptic(pattern) => format!("haptic:{pattern:?}"),
        }
    }

    /// Whether nothing plays on this route.
    #[must_use]
    pub const fn is_silent(&self) -> bool {
        matches!(self, Self::Silent(_))
    }
}

/// The route a platform has for a sound before the app's mute and the OS switches are asked.
const fn table_route(sound: SystemSound, platform: SoundPlatform) -> SoundRoute {
    match platform {
        SoundPlatform::Android => android_route(sound),
        SoundPlatform::Ios => ios_route(sound),
        SoundPlatform::MacOs
        | SoundPlatform::Windows
        | SoundPlatform::Linux
        | SoundPlatform::Other => SoundRoute::Silent(Silence::NoSystemRoute),
    }
}

/// An Android `AudioManager` sound effect, by the name of its `FX_*` constant.
const fn android_effect(name: &'static str, fallback: Option<&'static str>) -> SoundRoute {
    SoundRoute::Effect {
        platform: SoundPlatform::Android,
        name,
        fallback,
    }
}

/// Android: the sound effects of `AudioManager`, the user's notification tone and the camera
/// sounds. An effect is looked up by NAME at play time (the constants appeared across API levels).
const fn android_route(sound: SystemSound) -> SoundRoute {
    match sound {
        SystemSound::Click
        | SystemSound::ToggleOn
        | SystemSound::ToggleOff
        | SystemSound::TabChange
        | SystemSound::DropAccept => android_effect("FX_KEY_CLICK", None),
        SystemSound::Tick => android_effect("FX_FOCUS_NAVIGATION_DOWN", Some("FX_KEY_CLICK")),
        SystemSound::Back => android_effect("FX_BACK", Some("FX_KEY_CLICK")),
        SystemSound::Warning | SystemSound::Error | SystemSound::Bell | SystemSound::DropReject => {
            android_effect("FX_KEYPRESS_INVALID", None)
        }
        SystemSound::Notification
        | SystemSound::Success
        | SystemSound::Info
        | SystemSound::Question => SoundRoute::Ringtone {
            platform: SoundPlatform::Android,
            kind: "notification",
        },
        SystemSound::CameraShutter | SystemSound::ScreenCapture => SoundRoute::MediaAction {
            platform: SoundPlatform::Android,
            name: "SHUTTER_CLICK",
            only_if_required: false,
        },
        SystemSound::MenuOpen
        | SystemSound::MenuClose
        | SystemSound::Expand
        | SystemSound::Collapse
        | SystemSound::DragStart
        | SystemSound::Delete
        | SystemSound::TrashEmpty
        | SystemSound::Send => SoundRoute::Silent(Silence::NoSystemRoute),
    }
}

/// iOS: the haptic idiom. `AudioServicesPlaySystemSound` plays files the app bundles, and the
/// built-in ids are undocumented, so no sound is played.
const fn ios_route(sound: SystemSound) -> SoundRoute {
    match sound.haptic() {
        Some(pattern) => SoundRoute::Haptic(pattern),
        None => SoundRoute::Silent(Silence::NoSystemRoute),
    }
}

/// The route of `sound` on `platform`, after the ladder: the first sound of
/// [`SystemSound::fallback`]'s chain that has a route of its own.
const fn ladder_route(sound: SystemSound, platform: SoundPlatform) -> SoundRoute {
    let mut current = sound;
    loop {
        let route = table_route(current, platform);
        if !route.is_silent() {
            return route;
        }
        match current.fallback() {
            Some(next) => current = next,
            None => return route,
        }
    }
}

/// What plays for `sound` on `platform`, given the app's `config` and the OS's `audio` switches.
///
/// A pure function, so a host can test every cell of the table. The gates apply in this order:
///
/// 1. The app's mute ([`SoundConfig::enabled`]). On Android a camera sound survives it as
///    `only_if_required`: the OS says whether the region requires the sound.
/// 2. The OS switch of the sound's [`SoundClass`] (`audio`), for routes that play a sound. A
///    haptic is not a sound: the ring and silent switches do not silence it, and the system's own
///    haptics setting is applied by the platform call.
/// 3. The platform's table, walking [`SystemSound::fallback`] until a sound has a route.
#[must_use]
pub const fn route(
    sound: SystemSound,
    platform: SoundPlatform,
    config: SoundConfig,
    audio: AudioMetrics,
) -> SoundRoute {
    let table = ladder_route(sound, platform);
    if !config.enabled {
        return match table {
            SoundRoute::MediaAction { platform, name, .. } => SoundRoute::MediaAction {
                platform,
                name,
                only_if_required: true,
            },
            SoundRoute::Silent(_)
            | SoundRoute::Effect { .. }
            | SoundRoute::Ringtone { .. }
            | SoundRoute::Haptic(_) => SoundRoute::Silent(Silence::AppMuted),
        };
    }
    if matches!(table, SoundRoute::Silent(_) | SoundRoute::Haptic(_)) {
        return table;
    }
    // Android's `playSoundEffect` obeys "Touch sounds" for every id, also `FX_KEYPRESS_INVALID`,
    // which stands for an error here: the switch that applies is the one of the MECHANISM.
    let class = match table {
        SoundRoute::Effect {
            platform: SoundPlatform::Android,
            ..
        } => SoundClass::InputFeedback,
        SoundRoute::Silent(_)
        | SoundRoute::Effect { .. }
        | SoundRoute::Ringtone { .. }
        | SoundRoute::MediaAction { .. }
        | SoundRoute::Haptic(_) => sound.class(),
    };
    match class {
        SoundClass::InputFeedback if !audio.input_feedback_sounds_enabled => {
            SoundRoute::Silent(Silence::OsInputFeedbackOff)
        }
        SoundClass::Event if !audio.event_sounds_enabled => {
            SoundRoute::Silent(Silence::OsEventSoundsOff)
        }
        SoundClass::InputFeedback | SoundClass::Event => table,
    }
}

/// What happened to a request that was decided to play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundOutcome {
    /// Decided, and not handed to the platform yet.
    Pending,
    /// The platform call was made and accepted.
    Played,
    /// Not played on purpose, for this reason: `headless` (a headless run plays nothing),
    /// `AZ_SOUNDS=off`, or a gate the platform call itself found (`os:touch-sounds-off`).
    Skipped(String),
    /// The platform call failed; the text says how.
    Failed(String),
}

impl SoundOutcome {
    /// The outcome a platform helper reports as ONE line of text: `played`, `skipped:<why>` or
    /// `failed:<why>` (what [`SoundOutcome::label`] prints; the Android helper
    /// `AzulSounds.java` answers with it). Any other line is a failure that quotes it, so a
    /// helper that answers something unexpected is not taken for a sound that played.
    #[must_use]
    pub fn from_helper_line(line: &str) -> Self {
        let line = line.trim();
        if line == "played" {
            return Self::Played;
        }
        if let Some(why) = line.strip_prefix("skipped:") {
            return Self::Skipped(String::from(why));
        }
        if let Some(why) = line.strip_prefix("failed:") {
            return Self::Failed(String::from(why));
        }
        Self::Failed(format!("the platform helper answered {line:?}"))
    }

    /// The text of the outcome: `pending`, `played`, `skipped:<why>`, `failed:<why>`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Pending => String::from("pending"),
            Self::Played => String::from("played"),
            Self::Skipped(why) => format!("skipped:{why}"),
            Self::Failed(why) => format!("failed:{why}"),
        }
    }
}

/// One request, as the manager decided it: the recording `assert_sound`, the log and the sound
/// board read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundRecord {
    /// 1 for the first request of the window, then up by one per request.
    pub sequence: u64,
    /// What the app asked for.
    pub sound: SystemSound,
    /// What the platform has for it.
    pub route: SoundRoute,
    /// Why it did not play, when it did not: [`Silence::code`], [`COALESCED`], [`MIN_INTERVAL`]
    /// or [`REPEAT_LIMIT`]. `None` means the route was handed to the platform.
    pub suppressed_by: Option<&'static str>,
    /// What came of it.
    pub outcome: SoundOutcome,
}

impl SoundRecord {
    /// Whether the route was handed to the platform (nothing suppressed it).
    #[must_use]
    pub const fn was_admitted(&self) -> bool {
        self.suppressed_by.is_none()
    }

    /// One line: `#3 TabChange [input] route=system:android:FX_KEY_CLICK suppressed_by=- outcome=played`.
    #[must_use]
    pub fn describe(&self) -> String {
        let sequence = self.sequence;
        let sound = self.sound.name();
        let class = self.sound.class().name();
        let route = self.route.label();
        let suppressed_by = self.suppressed_by.unwrap_or("-");
        let outcome = self.outcome.label();
        format!(
            "#{sequence} {sound} [{class}] route={route} suppressed_by={suppressed_by} \
             outcome={outcome}"
        )
    }
}

/// A route the shell is to hand to the platform: what [`SoundManager::process`] admitted.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SoundDecision {
    /// The request's [`SoundRecord::sequence`], for [`SoundManager::record_outcome`].
    pub sequence: u64,
    /// The sound that was asked for.
    pub sound: SystemSound,
    /// What to play.
    pub route: SoundRoute,
}

/// A request that waits for the next drain.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct PendingSound {
    sound: SystemSound,
    /// The same sound was the request before this one in the same pass.
    coalesced: bool,
}

/// The last run of identical event sounds.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct Run {
    sound: SystemSound,
    count: u8,
    last_ms: u64,
}

/// The queue of sound requests of one window, the decision about each, and the recording.
///
/// The shape of [`crate::haptics::HapticManager`], with a decision and a recording added:
/// [`SoundManager::play`] queues what a callback asked for; the shell calls
/// [`SoundManager::process`] once per outermost event pass, plays what it returns and tells
/// [`SoundManager::record_outcome`] how that went. The recording is bounded
/// ([`MAX_SOUND_RECORDS`]) and is what `assert_sound` and the sound board read.
#[derive(Debug, Clone)]
pub struct SoundManager {
    config: SoundConfig,
    pending: Vec<PendingSound>,
    dropped: u32,
    records: VecDeque<SoundRecord>,
    next_sequence: u64,
    last_admitted_ms: [Option<u64>; SYSTEM_SOUND_COUNT],
    run: Option<Run>,
}

impl Default for SoundManager {
    fn default() -> Self {
        Self {
            config: SoundConfig::default(),
            pending: Vec::new(),
            dropped: 0,
            records: VecDeque::new(),
            next_sequence: 1,
            last_admitted_ms: [None; SYSTEM_SOUND_COUNT],
            run: None,
        }
    }
}

impl SoundManager {
    /// An empty manager with the default config: sounds on, widgets native.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What the app decided about sounds.
    #[must_use]
    pub const fn config(&self) -> SoundConfig {
        self.config
    }

    /// Replace the config.
    pub const fn set_config(&mut self, config: SoundConfig) {
        self.config = config;
    }

    /// Mute (`true`) or unmute every sound the app asks for.
    pub const fn set_muted(&mut self, muted: bool) {
        self.config.enabled = !muted;
    }

    /// Queue a sound. A request equal to the one before it in the queue is kept for the
    /// recording and merged into it when the queue is drained ([`COALESCED`]): a callback that
    /// fires per frame during a drag plays one sound, not sixty.
    pub fn play(&mut self, sound: SystemSound) {
        if self.pending.len() >= MAX_PENDING_SOUNDS {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let coalesced = self.pending.last().is_some_and(|last| last.sound == sound);
        self.pending.push(PendingSound { sound, coalesced });
    }

    /// Whether anything waits for the next drain.
    #[must_use]
    pub const fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// How many requests were dropped because the queue was full, since the last call.
    pub const fn take_dropped(&mut self) -> u32 {
        let dropped = self.dropped;
        self.dropped = 0;
        dropped
    }

    /// Decide every queued request: record it, and return the ones to play.
    ///
    /// `now_ms` is a monotonic clock in milliseconds. `Some` applies the pacing: a sound is
    /// dropped when the same sound played less than [`SystemSound::min_interval_ms`] ago
    /// ([`MIN_INTERVAL`]), and an event sound when it would be the third identical one in a row
    /// ([`REPEAT_LIMIT`]). `None` applies none of it: a headless run records every request.
    pub fn process(
        &mut self,
        platform: SoundPlatform,
        audio: AudioMetrics,
        now_ms: Option<u64>,
    ) -> Vec<SoundDecision> {
        let pending = core::mem::take(&mut self.pending);
        let mut admitted = Vec::new();
        for item in pending {
            let decided = route(item.sound, platform, self.config, audio);
            let suppressed_by = if item.coalesced {
                Some(COALESCED)
            } else if let SoundRoute::Silent(why) = decided {
                Some(why.code())
            } else {
                now_ms.and_then(|now| self.pace(item.sound, now))
            };
            let sequence = self.next_sequence;
            self.next_sequence = self.next_sequence.saturating_add(1);
            if self.records.len() >= MAX_SOUND_RECORDS {
                let _oldest = self.records.pop_front();
            }
            self.records.push_back(SoundRecord {
                sequence,
                sound: item.sound,
                route: decided,
                suppressed_by,
                outcome: suppressed_by.map_or(SoundOutcome::Pending, |why| {
                    SoundOutcome::Skipped(String::from(why))
                }),
            });
            if suppressed_by.is_none() {
                admitted.push(SoundDecision {
                    sequence,
                    sound: item.sound,
                    route: decided,
                });
            }
        }
        admitted
    }

    /// Queue nothing and play nothing: decide what is queued and mark what would have played as
    /// skipped for the reason `why`. What a headless run does, and a window with `AZ_SOUNDS=off`.
    pub fn record_without_playing(
        &mut self,
        platform: SoundPlatform,
        audio: AudioMetrics,
        now_ms: Option<u64>,
        why: &str,
    ) {
        for decision in self.process(platform, audio, now_ms) {
            self.record_outcome(decision.sequence, SoundOutcome::Skipped(String::from(why)));
        }
    }

    /// Say what came of a request [`SoundManager::process`] returned.
    pub fn record_outcome(&mut self, sequence: u64, outcome: SoundOutcome) {
        if let Some(record) = self
            .records
            .iter_mut()
            .rev()
            .find(|record| record.sequence == sequence)
        {
            record.outcome = outcome;
        }
    }

    /// The sequence number the next request gets: every record with a sequence from this one on
    /// belongs to the passes after this call.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// The recording, oldest first.
    pub fn records(&self) -> impl Iterator<Item = &SoundRecord> {
        self.records.iter()
    }

    /// How many requests the recording holds.
    #[must_use]
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// The most recent `limit` records as one line each, oldest first.
    #[must_use]
    pub fn describe_recent(&self, limit: usize) -> Vec<String> {
        let skip = self.records.len().saturating_sub(limit);
        self.records
            .iter()
            .skip(skip)
            .map(SoundRecord::describe)
            .collect()
    }

    /// Forget the recording (the requests stay numbered).
    pub fn clear_records(&mut self) {
        self.records.clear();
    }

    /// The pacing of one admitted sound: `Some(reason)` when it must not play now. Updates the
    /// state only for a sound that plays.
    fn pace(&mut self, sound: SystemSound, now_ms: u64) -> Option<&'static str> {
        let index = sound.index();
        if let Some(last) = self.last_admitted_ms[index] {
            if now_ms.saturating_sub(last) < sound.min_interval_ms() {
                return Some(MIN_INTERVAL);
            }
        }
        match sound.class() {
            SoundClass::Event => {
                let previous = self.run.filter(|run| {
                    run.sound == sound && now_ms.saturating_sub(run.last_ms) < RUN_GAP_MS
                });
                let count = previous.map_or(0, |run| run.count);
                if count >= MAX_IDENTICAL_IN_A_ROW {
                    return Some(REPEAT_LIMIT);
                }
                self.run = Some(Run {
                    sound,
                    count: count.saturating_add(1),
                    last_ms: now_ms,
                });
            }
            SoundClass::InputFeedback => self.run = None,
        }
        self.last_admitted_ms[index] = Some(now_ms);
        None
    }
}

#[cfg(test)]
#[path = "sound_test.rs"]
mod sound_test;
