//! AzMeet's keyboard: the table the settings list (azul-appkit's `Shortcut`) and the one rule
//! the window's key handler asks, so the list and the keys cannot drift apart. Pure: no azul
//! types, unit-tested here.

use azul_appkit::Shortcut;

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The microphone on or off.
    ToggleMic,
    /// The camera on or off.
    ToggleCamera,
    /// Back from the settings.
    CloseSettings,
}

/// The keys AzMeet reads (the window maps azul's `VirtualKeyCode` onto these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    D,
    E,
    Escape,
    Other,
}

/// Every shortcut, as the settings list them ("Mod" is Cmd on macOS, Ctrl elsewhere).
pub const SHORTCUTS: [Shortcut; 3] = [
    Shortcut::new("Call", "Mod+D", "Mute or unmute the microphone"),
    Shortcut::new("Call", "Mod+E", "Start or stop the camera"),
    Shortcut::new("Window", "Escape", "Close the settings"),
];

/// What `key` does; `primary`: the platform's shortcut modifier is held
/// (`KeyModifiers::primary_down`).
#[must_use]
pub fn command_for(key: Key, primary: bool) -> Option<Command> {
    // RED: no key does anything yet.
    let _ = (key, primary);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table entry's keys as the handler sees them.
    fn parse(keys: &str) -> (Key, bool) {
        let mut primary = false;
        let mut key = Key::Other;
        for part in keys.split('+') {
            match part {
                "Mod" => primary = true,
                "D" => key = Key::D,
                "E" => key = Key::E,
                "Escape" => key = Key::Escape,
                other => panic!("the table names a key the handler cannot see: {other}"),
            }
        }
        (key, primary)
    }

    #[test]
    fn every_listed_shortcut_runs_a_command_and_none_is_listed_twice() {
        for s in SHORTCUTS {
            let (key, primary) = parse(s.keys);
            assert!(command_for(key, primary).is_some(), "{} ({}) runs nothing", s.keys, s.action);
        }
        let mut keys: Vec<&str> = SHORTCUTS.iter().map(|s| s.keys).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), SHORTCUTS.len(), "a key listed twice");
    }

    #[test]
    fn the_call_keys_need_the_shortcut_modifier() {
        assert_eq!(command_for(Key::D, true), Some(Command::ToggleMic));
        assert_eq!(command_for(Key::E, true), Some(Command::ToggleCamera));
        assert_eq!(command_for(Key::D, false), None, "a plain D is typing");
        assert_eq!(command_for(Key::E, false), None);
        assert_eq!(command_for(Key::Escape, false), Some(Command::CloseSettings));
        assert_eq!(command_for(Key::Other, true), None);
    }
}
