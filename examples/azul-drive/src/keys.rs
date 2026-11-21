//! Explorer's keyboard as a table: which key with which modifiers runs which
//! command (Enter opens, Backspace / Alt+Up go up, Alt+Left / Alt+Right walk
//! the history, F2 renames, Delete / Shift+Delete, Ctrl+C / X / V, Ctrl+A,
//! Ctrl+Shift+N, F5, Ctrl+F / F3, the arrows with Shift / Ctrl, the context
//! menu key, type-ahead), Finder's Cmd keys beside them (Cmd+[ and Cmd+] walk
//! the history, Cmd+Up goes up, Cmd+Down opens, Cmd+Backspace deletes, Cmd+I
//! shows the properties), and the location tabs' keys (Ctrl+T, Ctrl+W,
//! Ctrl+Tab, Ctrl+1..9, Ctrl+Shift+A). Cmd counts as Ctrl on macOS for the
//! file commands; the tab keys are the Control key on every platform (Cmd+T
//! and Cmd+W too), so Cmd+1 can stay a layout on macOS. No azul types: the app
//! maps azul's key codes to [`Key`] and runs the [`Command`].

use azul_appkit::Shortcut;

/// A key, as far as the file view cares (the app maps azul's key codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A letter (lower case) or a digit.
    Char(char),
    Enter,
    Back,
    Delete,
    Escape,
    Tab,
    Space,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F2,
    F3,
    F5,
    F10,
    /// The context menu key.
    Apps,
    Other,
}

/// The modifiers held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Mods {
    pub shift: bool,
    /// The platform's shortcut modifier: Cmd on macOS, Ctrl elsewhere.
    pub ctrl: bool,
    pub alt: bool,
    /// The Control key itself, on every platform (on macOS a key apart from Cmd): the location
    /// tabs' keys read it, so Ctrl+1 jumps to a tab while Cmd+1 picks a layout.
    pub control: bool,
}

/// How far an arrow key moves in the visible order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    Prev,
    Next,
    PrevRow,
    NextRow,
    PageUp,
    PageDown,
    First,
    Last,
}

impl Step {
    /// The signed distance in items: a row of a grid is `columns` items,
    /// a page `page` items; First / Last are far enough to clamp.
    #[must_use]
    pub fn delta(self, columns: usize, page: usize) -> isize {
        let columns = columns.max(1) as isize;
        let page = page.max(1) as isize;
        match self {
            Step::Prev => -1,
            Step::Next => 1,
            Step::PrevRow => -columns,
            Step::NextRow => columns,
            Step::PageUp => -page,
            Step::PageDown => page,
            Step::First => isize::MIN / 2,
            Step::Last => isize::MAX / 2,
        }
    }
}

/// What a key does in the file view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    Open,
    Properties,
    Up,
    Back,
    Forward,
    Refresh,
    Search,
    Escape,
    Rename,
    Delete,
    DeletePermanently,
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    NewFolder,
    ContextMenu,
    /// An arrow / Home / End / Page key: Shift extends, Ctrl keeps the
    /// selection and moves only the focus.
    Move {
        step: Step,
        extend: bool,
        keep: bool,
    },
    /// Ctrl+Space.
    ToggleFocused,
    /// Ctrl+Shift+1..8: Explorer's layouts, Extra large icons (1) .. Content (8); on macOS
    /// Cmd+1 (large icons, 2) and Cmd+2 (details, 6) as in Finder.
    Layout(u8),
    /// Alt+P: the preview pane on or off (Explorer's key).
    PreviewPane,
    /// Alt+Shift+P: the details pane on or off (Explorer's key).
    DetailsPane,
    /// A letter or digit typed: jump to the name starting with it.
    TypeAhead(char),
    /// Ctrl+T (Cmd+T): a new location tab on the folder the window shows.
    NewTab,
    /// Ctrl+W (Cmd+W): the location tab showing closes (never the last one).
    CloseTab,
    /// Ctrl+Tab: the next location tab, round the end.
    NextTab,
    /// Ctrl+Shift+Tab: the previous location tab, round the start.
    PrevTab,
    /// Ctrl+1..9: the location tab of that number.
    JumpTab(u8),
    /// Ctrl+Shift+A: the tab overview.
    TabOverview,
    /// Space: Quick Look, a large preview of the focused item over the window (the arrows step
    /// through the files, Space or Escape closes it).
    QuickLook,
    /// Cmd+K: the command palette (folders, files, commands, a search of every drive).
    Palette,
}

/// The keyboard shortcuts as the Options' "Keyboard shortcuts" section lists them (azul-appkit's
/// table, `Mod` = Cmd on macOS, Ctrl elsewhere; the groups and actions are keys of the
/// resources). A test checks that every one of them runs a command in [`command_for`], so the
/// list cannot drift from the keys (DEDUP_OFFICE D13).
pub const SHORTCUTS: [Shortcut; 43] = [
    Shortcut::new("azdrive-shortcut-group-open", "Enter", "azdrive-shortcut-open-selected"),
    Shortcut::new("azdrive-shortcut-group-open", "Alt+Enter", "azdrive-shortcut-properties"),
    Shortcut::new("azdrive-shortcut-group-open", "Backspace", "azdrive-shortcut-up"),
    Shortcut::new("azdrive-shortcut-group-open", "Alt+Up", "azdrive-shortcut-up"),
    Shortcut::new("azdrive-shortcut-group-open", "Alt+Left", "azdrive-shortcut-back"),
    Shortcut::new("azdrive-shortcut-group-open", "Alt+Right", "azdrive-shortcut-forward"),
    Shortcut::new("azdrive-shortcut-group-open", "F5", "azdrive-shortcut-refresh"),
    Shortcut::new("azdrive-shortcut-group-open", "Mod+R", "azdrive-shortcut-refresh"),
    Shortcut::new("azdrive-shortcut-group-open", "Mod+F", "azdrive-shortcut-search"),
    Shortcut::new("azdrive-shortcut-group-open", "F3", "azdrive-shortcut-search"),
    Shortcut::new("azdrive-shortcut-group-open", "Mod+Down", "azdrive-shortcut-open-selected"),
    Shortcut::new("azdrive-shortcut-group-open", "Mod+I", "azdrive-shortcut-properties"),
    Shortcut::new("azdrive-shortcut-group-open", "Mod+Up", "azdrive-shortcut-up"),
    Shortcut::new("azdrive-shortcut-group-open", "Mod+[", "azdrive-shortcut-back"),
    Shortcut::new("azdrive-shortcut-group-open", "Mod+]", "azdrive-shortcut-forward"),
    Shortcut::new("azdrive-shortcut-group-organize", "F2", "azdrive-shortcut-rename"),
    Shortcut::new("azdrive-shortcut-group-organize", "Delete", "azdrive-shortcut-delete"),
    Shortcut::new("azdrive-shortcut-group-organize", "Mod+Backspace", "azdrive-shortcut-delete"),
    Shortcut::new(
        "azdrive-shortcut-group-organize",
        "Shift+Delete",
        "azdrive-shortcut-delete-for-good",
    ),
    Shortcut::new("azdrive-shortcut-group-organize", "Mod+C", "azdrive-shortcut-copy"),
    Shortcut::new("azdrive-shortcut-group-organize", "Mod+X", "azdrive-shortcut-cut"),
    Shortcut::new("azdrive-shortcut-group-organize", "Mod+V", "azdrive-shortcut-paste"),
    Shortcut::new("azdrive-shortcut-group-organize", "Mod+Z", "azdrive-shortcut-undo"),
    Shortcut::new("azdrive-shortcut-group-organize", "Mod+Shift+N", "azdrive-shortcut-new-folder"),
    Shortcut::new("azdrive-shortcut-group-select", "Mod+A", "azdrive-shortcut-select-all"),
    Shortcut::new("azdrive-shortcut-group-select", "Mod+Space", "azdrive-shortcut-toggle-focused"),
    Shortcut::new("azdrive-shortcut-group-select", "Shift+Down", "azdrive-shortcut-extend"),
    Shortcut::new("azdrive-shortcut-group-select", "Escape", "azdrive-shortcut-select-none"),
    Shortcut::new("azdrive-shortcut-group-select", "Shift+F10", "azdrive-shortcut-context-menu"),
    Shortcut::new("azdrive-shortcut-group-view", "Mod+Shift+2", "azdrive-shortcut-large-icons"),
    Shortcut::new("azdrive-shortcut-group-view", "Mod+Shift+5", "azdrive-shortcut-list"),
    Shortcut::new("azdrive-shortcut-group-view", "Mod+Shift+6", "azdrive-shortcut-details"),
    Shortcut::new("azdrive-shortcut-group-view", "Alt+P", "azdrive-shortcut-preview-pane"),
    Shortcut::new("azdrive-shortcut-group-view", "Alt+Shift+P", "azdrive-shortcut-details-pane"),
    Shortcut::new("azdrive-shortcut-group-view", "Mod+Alt+P", "azdrive-shortcut-preview-pane"),
    Shortcut::new("azdrive-shortcut-group-view", "Space", "azdrive-shortcut-quick-look"),
    Shortcut::new("azdrive-shortcut-group-view", "Mod+K", "azdrive-shortcut-palette"),
    Shortcut::new("azdrive-shortcut-group-tabs", "Ctrl+T", "azdrive-shortcut-new-tab"),
    Shortcut::new("azdrive-shortcut-group-tabs", "Ctrl+W", "azdrive-shortcut-close-tab"),
    Shortcut::new("azdrive-shortcut-group-tabs", "Ctrl+Tab", "azdrive-shortcut-next-tab"),
    Shortcut::new("azdrive-shortcut-group-tabs", "Ctrl+Shift+Tab", "azdrive-shortcut-previous-tab"),
    Shortcut::new("azdrive-shortcut-group-tabs", "Ctrl+1", "azdrive-shortcut-jump-tab"),
    Shortcut::new("azdrive-shortcut-group-tabs", "Ctrl+Shift+A", "azdrive-shortcut-tab-overview"),
];

/// The location tabs' keys: the Control key on every platform, and for Ctrl+T and Ctrl+W the
/// shortcut modifier too (Cmd+T, Cmd+W on macOS, as a browser has them).
fn tab_command(key: Key, mods: Mods) -> Option<Command> {
    let Mods {
        shift,
        ctrl,
        alt,
        control,
    } = mods;
    if alt {
        return None;
    }
    let command = match key {
        Key::Char('t') if (ctrl || control) && !shift => Command::NewTab,
        Key::Char('w') if (ctrl || control) && !shift => Command::CloseTab,
        Key::Tab if control && shift => Command::PrevTab,
        Key::Tab if control => Command::NextTab,
        Key::Char(c @ '1'..='9') if control && !shift => Command::JumpTab(c as u8 - b'0'),
        Key::Char('a') if control && shift => Command::TabOverview,
        _ => return None,
    };
    Some(command)
}

/// Explorer's keyboard, with Finder's Cmd keys and the location tabs' keys.
#[must_use]
pub fn command_for(key: Key, mods: Mods) -> Option<Command> {
    if let Some(command) = tab_command(key, mods) {
        return Some(command);
    }
    let Mods {
        shift,
        ctrl,
        alt,
        control,
    } = mods;
    let plain_ctrl = ctrl && !alt && !shift;
    let step = match key {
        Key::Up => Some(Step::PrevRow),
        Key::Down => Some(Step::NextRow),
        Key::Left => Some(Step::Prev),
        Key::Right => Some(Step::Next),
        Key::Home => Some(Step::First),
        Key::End => Some(Step::Last),
        Key::PageUp => Some(Step::PageUp),
        Key::PageDown => Some(Step::PageDown),
        _ => None,
    };
    let command = match key {
        Key::Enter if alt => Command::Properties,
        Key::Enter => Command::Open,
        // Finder's: Cmd+Up goes up, Cmd+Down opens, Cmd+Backspace deletes, Cmd+[ / Cmd+] walk
        // the history, Cmd+I shows the properties.
        Key::Up if plain_ctrl => Command::Up,
        Key::Down if plain_ctrl => Command::Open,
        Key::Back if plain_ctrl => Command::Delete,
        Key::Char('[') if plain_ctrl => Command::Back,
        Key::Char(']') if plain_ctrl => Command::Forward,
        Key::Char('i') if plain_ctrl => Command::Properties,
        Key::Back if !ctrl && !alt => Command::Up,
        Key::Up if alt => Command::Up,
        Key::Left if alt => Command::Back,
        Key::Right if alt => Command::Forward,
        Key::F5 => Command::Refresh,
        Key::F3 => Command::Search,
        Key::Escape => Command::Escape,
        Key::F2 => Command::Rename,
        Key::Delete if shift => Command::DeletePermanently,
        Key::Delete => Command::Delete,
        Key::Apps => Command::ContextMenu,
        Key::F10 if shift => Command::ContextMenu,
        Key::Space if ctrl => Command::ToggleFocused,
        Key::Space if !alt && !shift && !control => Command::QuickLook,
        Key::Char('k') if plain_ctrl => Command::Palette,
        Key::Char('n') if ctrl && shift && !alt => Command::NewFolder,
        Key::Char(c @ '1'..='8') if ctrl && shift && !alt => Command::Layout(c as u8 - b'0'),
        Key::Char('p') if alt && !ctrl && shift => Command::DetailsPane,
        Key::Char('p') if alt && !ctrl => Command::PreviewPane,
        Key::Char('p') if alt && ctrl && !shift => Command::PreviewPane,
        // Finder's Cmd+1 and Cmd+2 (macOS only: where Cmd is Ctrl, Ctrl+1 is a tab key).
        Key::Char('1') if plain_ctrl && !control => Command::Layout(2),
        Key::Char('2') if plain_ctrl && !control => Command::Layout(6),
        Key::Char('r') if plain_ctrl => Command::Refresh,
        Key::Char('f' | 'e') if plain_ctrl => Command::Search,
        Key::Char('c') if plain_ctrl => Command::Copy,
        Key::Char('x') if plain_ctrl => Command::Cut,
        Key::Char('v') if plain_ctrl => Command::Paste,
        Key::Char('a') if plain_ctrl => Command::SelectAll,
        Key::Char('z') if plain_ctrl => Command::Undo,
        Key::Char(c) if !ctrl && !alt && !control => Command::TypeAhead(c),
        _ => match step {
            Some(step) if !alt => Command::Move {
                step,
                extend: shift,
                keep: ctrl,
            },
            _ => return None,
        },
    };
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
        control: false,
    };
    const CTRL: Mods = Mods {
        shift: false,
        ctrl: true,
        alt: false,
        control: false,
    };
    const SHIFT: Mods = Mods {
        shift: true,
        ctrl: false,
        alt: false,
        control: false,
    };
    const ALT: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: true,
        control: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        shift: true,
        ctrl: true,
        alt: false,
        control: false,
    };
    /// The Control key on macOS: not the shortcut modifier there.
    const CONTROL: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
        control: true,
    };
    const CONTROL_SHIFT: Mods = Mods {
        shift: true,
        ctrl: false,
        alt: false,
        control: true,
    };
    /// Ctrl on Windows and Linux: the Control key IS the shortcut modifier.
    const PC_CTRL: Mods = Mods {
        shift: false,
        ctrl: true,
        alt: false,
        control: true,
    };

    #[test]
    fn explorers_navigation_keys() {
        assert_eq!(command_for(Key::Enter, NONE), Some(Command::Open));
        assert_eq!(command_for(Key::Enter, ALT), Some(Command::Properties));
        assert_eq!(command_for(Key::Back, NONE), Some(Command::Up));
        assert_eq!(command_for(Key::Up, ALT), Some(Command::Up));
        assert_eq!(command_for(Key::Left, ALT), Some(Command::Back));
        assert_eq!(command_for(Key::Right, ALT), Some(Command::Forward));
        assert_eq!(command_for(Key::F5, NONE), Some(Command::Refresh));
        assert_eq!(command_for(Key::Char('r'), CTRL), Some(Command::Refresh));
        assert_eq!(command_for(Key::Char('f'), CTRL), Some(Command::Search));
        assert_eq!(command_for(Key::F3, NONE), Some(Command::Search));
        assert_eq!(command_for(Key::Escape, NONE), Some(Command::Escape));
    }

    #[test]
    fn explorers_editing_keys() {
        assert_eq!(command_for(Key::F2, NONE), Some(Command::Rename));
        assert_eq!(command_for(Key::Delete, NONE), Some(Command::Delete));
        assert_eq!(
            command_for(Key::Delete, SHIFT),
            Some(Command::DeletePermanently)
        );
        assert_eq!(command_for(Key::Char('c'), CTRL), Some(Command::Copy));
        assert_eq!(command_for(Key::Char('x'), CTRL), Some(Command::Cut));
        assert_eq!(command_for(Key::Char('v'), CTRL), Some(Command::Paste));
        assert_eq!(command_for(Key::Char('a'), CTRL), Some(Command::SelectAll));
        assert_eq!(command_for(Key::Char('z'), CTRL), Some(Command::Undo));
        assert_eq!(
            command_for(Key::Char('n'), CTRL_SHIFT),
            Some(Command::NewFolder)
        );
        assert_eq!(command_for(Key::Apps, NONE), Some(Command::ContextMenu));
        assert_eq!(command_for(Key::F10, SHIFT), Some(Command::ContextMenu));
    }

    #[test]
    fn the_arrows_move_shift_extends_ctrl_keeps_and_home_end_jump() {
        assert_eq!(
            command_for(Key::Down, NONE),
            Some(Command::Move {
                step: Step::NextRow,
                extend: false,
                keep: false
            })
        );
        assert_eq!(
            command_for(Key::Up, SHIFT),
            Some(Command::Move {
                step: Step::PrevRow,
                extend: true,
                keep: false
            })
        );
        assert_eq!(
            command_for(Key::Right, CTRL),
            Some(Command::Move {
                step: Step::Next,
                extend: false,
                keep: true
            })
        );
        assert_eq!(
            command_for(Key::End, SHIFT),
            Some(Command::Move {
                step: Step::Last,
                extend: true,
                keep: false
            })
        );
        assert_eq!(command_for(Key::Space, CTRL), Some(Command::ToggleFocused));
    }

    #[test]
    fn plain_letters_and_digits_type_ahead_and_modified_ones_do_not() {
        assert_eq!(
            command_for(Key::Char('q'), NONE),
            Some(Command::TypeAhead('q'))
        );
        assert_eq!(
            command_for(Key::Char('7'), SHIFT),
            Some(Command::TypeAhead('7'))
        );
        assert_eq!(command_for(Key::Char('q'), CTRL), None);
        assert_eq!(command_for(Key::Char('q'), ALT), None);
        assert_eq!(command_for(Key::Tab, NONE), None, "Tab moves the focus");
    }

    /// Explorer's layout keys: Ctrl+Shift+1 Extra large icons .. 8 Content.
    #[test]
    fn ctrl_shift_and_a_digit_pick_one_of_the_eight_layouts() {
        for digit in 1..=8u8 {
            let c = char::from(b'0' + digit);
            assert_eq!(
                command_for(Key::Char(c), CTRL_SHIFT),
                Some(Command::Layout(digit)),
                "{c}"
            );
        }
        assert_eq!(command_for(Key::Char('9'), CTRL_SHIFT), None);
        assert_eq!(command_for(Key::Char('0'), CTRL_SHIFT), None);
    }

    /// Explorer's pane keys: Alt+P the preview pane, Alt+Shift+P the details pane; a plain p
    /// still types ahead.
    #[test]
    fn alt_p_toggles_the_preview_pane_and_alt_shift_p_the_details_pane() {
        const ALT_SHIFT: Mods = Mods {
            shift: true,
            ctrl: false,
            alt: true,
            control: false,
        };
        assert_eq!(command_for(Key::Char('p'), ALT), Some(Command::PreviewPane));
        assert_eq!(
            command_for(Key::Char('p'), ALT_SHIFT),
            Some(Command::DetailsPane)
        );
        assert_eq!(
            command_for(Key::Char('p'), NONE),
            Some(Command::TypeAhead('p'))
        );
    }

    /// The location tabs: Ctrl+T and Ctrl+W with the Control key or the shortcut modifier
    /// (Cmd+T, Cmd+W on macOS), Ctrl+Tab and Ctrl+Shift+Tab, Ctrl+1..9, Ctrl+Shift+A - on
    /// macOS with the Control key, on Windows and Linux with Ctrl.
    #[test]
    fn the_location_tab_keys_work_with_control_on_every_platform() {
        for mods in [CONTROL, CTRL, PC_CTRL] {
            assert_eq!(command_for(Key::Char('t'), mods), Some(Command::NewTab), "{mods:?}");
            assert_eq!(command_for(Key::Char('w'), mods), Some(Command::CloseTab), "{mods:?}");
        }
        for mods in [CONTROL, PC_CTRL] {
            assert_eq!(command_for(Key::Tab, mods), Some(Command::NextTab), "{mods:?}");
            assert_eq!(command_for(Key::Char('1'), mods), Some(Command::JumpTab(1)), "{mods:?}");
            assert_eq!(command_for(Key::Char('9'), mods), Some(Command::JumpTab(9)), "{mods:?}");
        }
        assert_eq!(command_for(Key::Tab, CONTROL_SHIFT), Some(Command::PrevTab));
        assert_eq!(command_for(Key::Char('a'), CONTROL_SHIFT), Some(Command::TabOverview));
        let pc_ctrl_shift = Mods {
            shift: true,
            ..PC_CTRL
        };
        assert_eq!(command_for(Key::Tab, pc_ctrl_shift), Some(Command::PrevTab));
        assert_eq!(command_for(Key::Char('a'), pc_ctrl_shift), Some(Command::TabOverview));
        assert_eq!(command_for(Key::Char('a'), PC_CTRL), Some(Command::SelectAll), "Ctrl+A stays");
        assert_eq!(command_for(Key::Tab, CTRL), None, "Cmd+Tab is the system's app switcher");
        assert_eq!(command_for(Key::Char('0'), CONTROL), None, "no tab 0");
        assert_eq!(
            command_for(Key::Char('t'), CONTROL_SHIFT),
            None,
            "Ctrl+Shift+T is no new tab"
        );
    }

    /// A letter with the Control key held is never type-ahead (on macOS Control is no
    /// shortcut modifier, so the letter would otherwise jump to a name).
    #[test]
    fn a_letter_with_control_held_does_not_type_ahead() {
        assert_eq!(command_for(Key::Char('q'), CONTROL), None);
        assert_eq!(command_for(Key::Char('5'), CONTROL_SHIFT), None);
    }

    /// Finder's keys beside Explorer's: Cmd+[ and Cmd+] go back and forward, Cmd+Up goes up,
    /// Cmd+Down opens, Cmd+Backspace deletes, Cmd+I shows the properties, Cmd+Alt+P the preview
    /// pane; Cmd+Left and Cmd+Right still move only the focus.
    #[test]
    fn finders_cmd_keys_walk_the_history_go_up_open_delete_and_show_the_properties() {
        let cmd_alt = Mods { alt: true, ..CTRL };
        assert_eq!(command_for(Key::Char('['), CTRL), Some(Command::Back));
        assert_eq!(command_for(Key::Char(']'), CTRL), Some(Command::Forward));
        assert_eq!(command_for(Key::Up, CTRL), Some(Command::Up));
        assert_eq!(command_for(Key::Down, CTRL), Some(Command::Open));
        assert_eq!(command_for(Key::Back, CTRL), Some(Command::Delete));
        assert_eq!(command_for(Key::Char('i'), CTRL), Some(Command::Properties));
        assert_eq!(command_for(Key::Char('p'), cmd_alt), Some(Command::PreviewPane));
        assert_eq!(
            command_for(Key::Left, CTRL),
            Some(Command::Move {
                step: Step::Prev,
                extend: false,
                keep: true
            })
        );
    }

    /// Cmd+1 large icons and Cmd+2 details on macOS (Finder's keys); on Windows and Linux
    /// Ctrl+1 and Ctrl+2 are the tab keys, and Ctrl+Shift+1..8 the layouts.
    #[test]
    fn cmd_one_and_two_pick_icons_and_details_where_ctrl_one_is_not_a_tab_key() {
        assert_eq!(command_for(Key::Char('1'), CTRL), Some(Command::Layout(2)));
        assert_eq!(command_for(Key::Char('2'), CTRL), Some(Command::Layout(6)));
        assert_eq!(command_for(Key::Char('1'), PC_CTRL), Some(Command::JumpTab(1)));
    }

    /// Space alone is Quick Look (Ctrl+Space still toggles the focused item); Cmd+K opens the
    /// command palette.
    #[test]
    fn space_opens_quick_look_and_cmd_k_the_command_palette() {
        assert_eq!(command_for(Key::Space, NONE), Some(Command::QuickLook));
        assert_eq!(command_for(Key::Space, CTRL), Some(Command::ToggleFocused));
        assert_eq!(command_for(Key::Char('k'), CTRL), Some(Command::Palette));
        assert_eq!(command_for(Key::Char('k'), PC_CTRL), Some(Command::Palette));
        assert_eq!(command_for(Key::Char('k'), NONE), Some(Command::TypeAhead('k')));
    }

    /// `Mod+Shift+N` -> (`Char('n')`, Ctrl + Shift), as the table spells keys; `Ctrl` is the
    /// Control key.
    fn parse_keys(keys: &str) -> (Key, Mods) {
        let mut mods = NONE;
        let mut key = Key::Other;
        for part in keys.split('+') {
            match part {
                "Mod" => mods.ctrl = true,
                "Ctrl" => mods.control = true,
                "Shift" => mods.shift = true,
                "Alt" => mods.alt = true,
                "Enter" => key = Key::Enter,
                "Backspace" => key = Key::Back,
                "Delete" => key = Key::Delete,
                "Escape" => key = Key::Escape,
                "Space" => key = Key::Space,
                "Tab" => key = Key::Tab,
                "Up" => key = Key::Up,
                "Down" => key = Key::Down,
                "Left" => key = Key::Left,
                "Right" => key = Key::Right,
                "F2" => key = Key::F2,
                "F3" => key = Key::F3,
                "F5" => key = Key::F5,
                "F10" => key = Key::F10,
                one if one.chars().count() == 1 => {
                    key = Key::Char(one.chars().next().unwrap().to_ascii_lowercase());
                }
                other => panic!("the table spells a key this test does not know: {other}"),
            }
        }
        (key, mods)
    }

    #[test]
    fn every_listed_shortcut_runs_a_command_and_none_is_listed_twice() {
        for s in SHORTCUTS {
            let (key, mods) = parse_keys(s.keys);
            let command = command_for(key, mods);
            assert!(
                command.is_some() && !matches!(command, Some(Command::TypeAhead(_))),
                "{} ({}) runs nothing",
                s.keys,
                s.action
            );
        }
        let mut keys: Vec<&str> = SHORTCUTS.iter().map(|s| s.keys).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), SHORTCUTS.len(), "a key listed twice");
        assert_eq!(
            command_for(parse_keys("Mod+Shift+6").0, parse_keys("Mod+Shift+6").1),
            Some(Command::Layout(6))
        );
    }

    #[test]
    fn a_step_is_a_signed_distance_in_the_visible_order() {
        assert_eq!(Step::Next.delta(4, 10), 1);
        assert_eq!(Step::Prev.delta(4, 10), -1);
        assert_eq!(Step::NextRow.delta(4, 10), 4, "a grid moves a row");
        assert_eq!(Step::PrevRow.delta(1, 10), -1, "a list row is one item");
        assert_eq!(Step::PageDown.delta(1, 10), 10);
        assert!(Step::First.delta(4, 10) < -1_000_000);
        assert!(Step::Last.delta(4, 10) > 1_000_000);
    }
}
