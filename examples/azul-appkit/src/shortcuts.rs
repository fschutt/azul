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
    Shortcut::new("Window", "Mod+,", "Open the settings"),
    Shortcut::new("Window", "F1", "Show the keyboard shortcuts"),
    Shortcut::new("Window", "Escape", "Cancel the settings (close them, the changes undone)"),
];

/// The keys as the user reads them on this platform: `Mod` is `Cmd` on macOS
/// and `Ctrl` elsewhere; the `+` joins stay.
#[must_use]
pub fn display_keys(keys: &str, mac: bool) -> String {
    keys.split('+')
        .map(|part| match part {
            "Mod" => {
                if mac {
                    "Cmd"
                } else {
                    "Ctrl"
                }
            }
            "Alt" if mac => "Option",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+")
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
    let hay = format!(
        "{} {} {} {}",
        s.group,
        s.keys,
        display_keys(s.keys, mac),
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
    fn mod_reads_cmd_on_macos_and_ctrl_elsewhere() {
        assert_eq!(display_keys("Mod+C", true), "Cmd+C");
        assert_eq!(display_keys("Mod+C", false), "Ctrl+C");
        assert_eq!(display_keys("Alt+1", true), "Option+1");
        assert_eq!(display_keys("Alt+1", false), "Alt+1");
        assert_eq!(display_keys("Shift+F6", true), "Shift+F6");
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
