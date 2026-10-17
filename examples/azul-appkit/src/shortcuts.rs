//! The keyboard-shortcut table every app shows on its settings page and its
//! help (build ledger F6).
//!
//! An app lists its shortcuts once, as data: the group, the keys and the
//! action. `Mod` stands for the platform's command modifier (Cmd on macOS,
//! Ctrl elsewhere), so one table serves every desktop.

/// One shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// The heading it is listed under ("General", "Editing").
    pub group: &'static str,
    /// The keys, `+`-joined: `Mod+C`, `Shift+F6`, `Enter`.
    pub keys: &'static str,
    /// What it does, as a sentence fragment: "Copy the result".
    pub action: &'static str,
}

impl Shortcut {
    /// A shortcut.
    #[must_use]
    pub const fn new(group: &'static str, keys: &'static str, action: &'static str) -> Self {
        Shortcut {
            group,
            keys,
            action,
        }
    }
}

/// The shortcuts every app built on the kit has.
pub const KIT_SHORTCUTS: [Shortcut; 3] = [
    Shortcut::new("kit-shortcut-group-window", "Mod+,", "kit-shortcut-settings"),
    Shortcut::new("kit-shortcut-group-window", "F1", "kit-shortcut-shortcuts"),
    Shortcut::new("kit-shortcut-group-window", "Escape", "kit-shortcut-cancel-settings"),
];

/// The modifiers in the order macOS writes them, with their symbols.
const MAC_MODIFIERS: [(&str, &str); 4] = [
    ("Ctrl", "\u{2303}"),
    ("Alt", "\u{2325}"),
    ("Shift", "\u{21e7}"),
    ("Mod", "\u{2318}"),
];

/// The keys a Mac keyboard prints a symbol on.
const MAC_KEYS: [(&str, &str); 14] = [
    ("Escape", "\u{238b}"),
    ("Enter", "\u{21a9}"),
    ("Return", "\u{21a9}"),
    ("Tab", "\u{21e5}"),
    ("Backspace", "\u{232b}"),
    ("Delete", "\u{2326}"),
    ("Up", "\u{2191}"),
    ("Down", "\u{2193}"),
    ("Left", "\u{2190}"),
    ("Right", "\u{2192}"),
    ("PageUp", "\u{21de}"),
    ("PageDown", "\u{21df}"),
    ("Home", "\u{2196}"),
    ("End", "\u{2198}"),
];

/// [`display_keys`] with every key name said by `name` (the window's language: `Ctrl` is
/// `Strg` in German). The Mac's symbols are no names and stay.
#[must_use]
pub fn display_keys_with(keys: &str, mac: bool, name: &dyn Fn(&str) -> String) -> String {
    let parts: Vec<&str> = keys.split('+').collect();
    if !mac {
        return parts
            .iter()
            .map(|part| name(if *part == "Mod" { "Ctrl" } else { part }))
            .collect::<Vec<_>>()
            .join("+");
    }
    let mut out = String::new();
    for (modifier, symbol) in MAC_MODIFIERS {
        if parts.contains(&modifier) {
            out.push_str(symbol);
        }
    }
    for part in parts.iter().filter(|p| !MAC_MODIFIERS.iter().any(|(m, _)| m == *p)) {
        match MAC_KEYS.iter().find(|(k, _)| k == part) {
            Some((_, symbol)) => out.push_str(symbol),
            None => out.push_str(&name(part)),
        }
    }
    out
}

/// The keys as the user reads them on this platform: on macOS the modifiers' symbols in
/// Apple's order before the key (`\u{21e7}\u{2318}S`), elsewhere the names joined by `+`
/// (`Mod` is `Ctrl`).
#[must_use]
pub fn display_keys(keys: &str, mac: bool) -> String {
    display_keys_with(keys, mac, &|part: &str| part.to_string())
}

/// The groups in the order they first appear, each with its shortcuts in table order.
#[must_use]
pub fn groups(list: &[Shortcut]) -> Vec<(&'static str, Vec<Shortcut>)> {
    let mut out: Vec<(&'static str, Vec<Shortcut>)> = Vec::new();
    for s in list {
        match out.iter_mut().find(|(g, _)| *g == s.group) {
            Some((_, items)) => items.push(*s),
            None => out.push((s.group, vec![*s])),
        }
    }
    out
}

/// Whether a shortcut matches a search: every word of the query appears in
/// its group, keys or action (any case). An empty query matches everything.
#[must_use]
pub fn matches(s: &Shortcut, query: &str, mac: bool) -> bool {
    // The modifiers' names too: a Mac shows symbols, and "cmd" must still find Cmd+C.
    let spoken = if mac {
        s.keys.replace("Mod", "Cmd").replace("Alt", "Option")
    } else {
        s.keys.replace("Mod", "Ctrl")
    };
    let hay = format!(
        "{} {} {} {} {}",
        s.group,
        s.keys,
        display_keys(s.keys, mac),
        spoken,
        s.action
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| hay.contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: [Shortcut; 4] = [
        Shortcut::new("General", "Mod+C", "Copy the result"),
        Shortcut::new("Keypad", "Enter", "Evaluate"),
        Shortcut::new("General", "Mod+V", "Paste a number"),
        Shortcut::new("Keypad", "Alt+1", "Standard mode"),
    ];

    #[test]
    fn mod_reads_ctrl_elsewhere() {
        assert_eq!(display_keys("Mod+C", false), "Ctrl+C");
        assert_eq!(display_keys("Alt+1", false), "Alt+1");
        assert_eq!(display_keys("Shift+F6", false), "Shift+F6");
    }

    /// macOS writes the modifiers as their symbols, in Apple's order (Control, Option, Shift,
    /// Command), before the key and without a +; the keys with a symbol on the keyboard
    /// (Escape, Return, the arrows) take it too.
    #[test]
    fn macos_shows_the_modifier_symbols_in_apples_order() {
        assert_eq!(display_keys("Mod+C", true), "\u{2318}C");
        assert_eq!(display_keys("Mod+Shift+S", true), "\u{21e7}\u{2318}S");
        assert_eq!(display_keys("Alt+1", true), "\u{2325}1");
        assert_eq!(display_keys("Ctrl+Tab", true), "\u{2303}\u{21e5}");
        assert_eq!(display_keys("Mod+Escape", true), "\u{2318}\u{238b}");
        assert_eq!(display_keys("Shift+F6", true), "\u{21e7}F6");
        assert_eq!(display_keys("Mod+Space", true), "\u{2318}Space");
    }

    /// The names are the window's language's (`name` translates one), the symbols are not.
    #[test]
    fn key_names_are_said_by_the_caller_and_symbols_stay() {
        let german = |part: &str| match part {
            "Ctrl" => String::from("Strg"),
            "Escape" => String::from("Esc"),
            "Space" => String::from("Leertaste"),
            other => other.to_string(),
        };
        assert_eq!(display_keys_with("Mod+Escape", false, &german), "Strg+Esc");
        assert_eq!(display_keys_with("Mod+Space", true, &german), "\u{2318}Leertaste");
    }

    #[test]
    fn groups_keep_the_order_they_first_appear_in() {
        let g = groups(&TABLE);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, "General");
        assert_eq!(
            g[0].1.iter().map(|s| s.keys).collect::<Vec<_>>(),
            vec!["Mod+C", "Mod+V"]
        );
        assert_eq!(g[1].0, "Keypad");
    }

    #[test]
    fn a_search_matches_every_word_against_group_keys_and_action() {
        assert!(matches(&TABLE[0], "", false));
        assert!(matches(&TABLE[0], "copy", false));
        assert!(matches(&TABLE[0], "ctrl c", false));
        assert!(matches(&TABLE[0], "cmd", true));
        assert!(!matches(&TABLE[0], "cmd", false), "Ctrl on this platform");
        assert!(!matches(&TABLE[1], "copy", false));
    }

    #[test]
    fn the_kit_shortcuts_open_and_close_the_settings() {
        assert!(KIT_SHORTCUTS.iter().any(|s| s.keys == "Mod+,"));
        assert!(KIT_SHORTCUTS.iter().any(|s| s.keys == "Escape"));
    }
}
