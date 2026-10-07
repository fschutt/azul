//! AzMeet's keyboard: the table the settings list (azul-appkit's `Shortcut`) and the one rule
//! the window's key handler asks, so the list and the keys cannot drift apart. The window's keys
//! (Mod+, opens the settings, F1 the shortcuts, Escape closes them) are the kit's
//! (`azul_appkit::ui::handle_key`, asked first). Pure: no azul types, unit-tested here.

use azul_appkit::Shortcut;

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The microphone on or off.
    ToggleMic,
    /// The camera on or off.
    ToggleCamera,
}

/// The keys AzMeet reads (the window maps azul's `VirtualKeyCode` onto these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    D,
    E,
    Other,
}

/// AzMeet's own shortcuts, as the settings list them ("Mod" is Cmd on macOS, Ctrl elsewhere);
/// the kit lists its window keys after them.
pub const SHORTCUTS: [Shortcut; 2] = [
    Shortcut::new("Call", "Mod+D", "Mute or unmute the microphone"),
    Shortcut::new("Call", "Mod+E", "Start or stop the camera"),
];

/// What `key` does; `primary`: the platform's shortcut modifier is held
/// (`KeyModifiers::primary_down`).
#[must_use]
pub fn command_for(key: Key, primary: bool) -> Option<Command> {
    match key {
        Key::D if primary => Some(Command::ToggleMic),
        Key::E if primary => Some(Command::ToggleCamera),
        _ => None,
    }
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
        keys.extend(azul_appkit::shortcuts::KIT_SHORTCUTS.iter().map(|s| s.keys));
        let all = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), all, "a key listed twice, here or beside the kit's");
    }

    #[test]
    fn the_call_keys_need_the_shortcut_modifier() {
        assert_eq!(command_for(Key::D, true), Some(Command::ToggleMic));
        assert_eq!(command_for(Key::E, true), Some(Command::ToggleCamera));
        assert_eq!(command_for(Key::D, false), None, "a plain D is typing");
        assert_eq!(command_for(Key::E, false), None);
        assert_eq!(command_for(Key::Other, true), None);
    }
}
