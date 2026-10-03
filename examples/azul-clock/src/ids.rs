//! AzClock's DOM ids and classes, each name defined ONCE, every one carrying
//! the app's `__azclock_` prefix (user ruling 2026-10-02, like the widgets'
//! `__azul_`): no clash with a widget's or another app's names, and no string
//! copied at run time for a fixed name.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azclock_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azclock_", $value));)*
    };
}

names! {
    // ---- the mode row ----
    /// The World / Alarms / Timer / Stopwatch switch.
    MODES = "modes";
    SETTINGS = "settings";
    /// The line under the content: the next alarm, a notice.
    STATUS = "status";
    NOTICE = "notice";
}

/// A name made at run time from a part defined once elsewhere (an alarm's
/// or a city's index): the prefix, then `suffix`.
#[must_use]
pub fn named(suffix: &str) -> AzString {
    AzString::from(format!("{PREFIX}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_app_prefix() {
        for name in [MODES, SETTINGS, STATUS, NOTICE] {
            assert!(name.as_str().starts_with(PREFIX), "{}", name.as_str());
        }
        assert_eq!(named("alarm-0").as_str(), "__azclock_alarm-0");
    }
}
