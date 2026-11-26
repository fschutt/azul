//! Where the phone's screens are drawn: the size of the window, the safe area (the notch, the home indicator, the
//! system bars) and the platform. On a phone this is read from the window at each layout pass; the simulator picks a
//! [`Device`] and says what that phone would report. The screens are built from an [`Env`] and nothing else, so one
//! code path serves the phone and the simulator.

/// The margins the system keeps for its own things, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Insets {
    pub const NONE: Insets = Insets { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };

    /// The insets, each held between 0 and a quarter of the window (a shell that reports nonsense does not eat the screen).
    #[must_use]
    pub fn held(self, width: f32, height: f32) -> Insets {
        let hold = |v: f32, max: f32| if v.is_finite() { v.clamp(0.0, max) } else { 0.0 };
        Insets {
            top: hold(self.top, height / 4.0),
            right: hold(self.right, width / 4.0),
            bottom: hold(self.bottom, height / 4.0),
            left: hold(self.left, width / 4.0),
        }
    }
}

/// Which system the phone runs (the permission texts and the system bars differ).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Ios,
    Android,
}

/// The words the app speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    /// The language of a locale tag (`de-DE`, `de`, `en-US`): German or else English.
    #[must_use]
    pub fn from_tag(tag: &str) -> Lang {
        if tag.trim().to_ascii_lowercase().starts_with("de") {
            Lang::De
        } else {
            Lang::En
        }
    }

    /// The tag the engine's locale takes.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Lang::En => "en-US",
            Lang::De => "de-DE",
        }
    }
}

/// What the screens are built for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Env {
    pub width: f32,
    pub height: f32,
    pub insets: Insets,
    pub platform: Platform,
    pub lang: Lang,
}

impl Env {
    /// Wider than tall.
    #[must_use]
    pub fn landscape(&self) -> bool {
        self.width > self.height
    }

    /// The room inside the safe area.
    #[must_use]
    pub fn usable(&self) -> (f32, f32) {
        (
            (self.width - self.insets.left - self.insets.right).max(0.0),
            (self.height - self.insets.top - self.insets.bottom).max(0.0),
        )
    }

    /// A short phone (an iPhone SE, a phone in landscape): the screens give up some padding.
    #[must_use]
    pub fn short(&self) -> bool {
        self.usable().1 < 640.0
    }
}

/// A phone the simulator can be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Device {
    pub id: &'static str,
    pub name: &'static str,
    /// Portrait size in logical pixels.
    pub width: f32,
    pub height: f32,
    pub platform: Platform,
    pub portrait: Insets,
    pub landscape: Insets,
}

/// The phones of the layout study: a current iPhone, a small Android and a large one.
pub static DEVICES: [Device; 3] = [
    Device {
        id: "iphone",
        name: "iPhone 15",
        width: 393.0,
        height: 852.0,
        platform: Platform::Ios,
        portrait: Insets { top: 59.0, right: 0.0, bottom: 34.0, left: 0.0 },
        landscape: Insets { top: 0.0, right: 59.0, bottom: 21.0, left: 59.0 },
    },
    Device {
        id: "android-s",
        name: "Android small",
        width: 360.0,
        height: 800.0,
        platform: Platform::Android,
        portrait: Insets { top: 28.0, right: 0.0, bottom: 24.0, left: 0.0 },
        landscape: Insets { top: 24.0, right: 0.0, bottom: 24.0, left: 0.0 },
    },
    Device {
        id: "android-l",
        name: "Android large",
        width: 412.0,
        height: 915.0,
        platform: Platform::Android,
        portrait: Insets { top: 32.0, right: 0.0, bottom: 24.0, left: 0.0 },
        landscape: Insets { top: 24.0, right: 0.0, bottom: 24.0, left: 0.0 },
    },
];

impl Device {
    /// What this phone reports in `landscape` or portrait, in `lang`; `safe` false drops the insets (a phone with
    /// no notch, or a window that is only a window).
    #[must_use]
    pub fn env(&self, landscape: bool, lang: Lang, safe: bool) -> Env {
        let (width, height) = if landscape { (self.height, self.width) } else { (self.width, self.height) };
        let insets = if !safe {
            Insets::NONE
        } else if landscape {
            self.landscape
        } else {
            self.portrait
        };
        Env { width, height, insets, platform: self.platform, lang }
    }

    /// The device with this id.
    #[must_use]
    pub fn by_id(id: &str) -> Option<&'static Device> {
        DEVICES.iter().find(|d| d.id == id)
    }
}

#[cfg(test)]
#[path = "env_test.rs"]
mod env_test;
