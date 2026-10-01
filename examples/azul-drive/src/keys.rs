//! Explorer's keyboard as a table: which key with which modifiers runs which
//! command (Enter opens, Backspace / Alt+Up go up, Alt+Left / Alt+Right walk
//! the history, F2 renames, Delete / Shift+Delete, Ctrl+C / X / V, Ctrl+A,
//! Ctrl+Shift+N, F5, Ctrl+F / F3, the arrows with Shift / Ctrl, the context
//! menu key, type-ahead). Cmd counts as Ctrl on macOS. No azul types: the app
//! maps azul's key codes to [`Key`] and runs the [`Command`].

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

/// The modifiers held (`ctrl` is Ctrl or, on macOS, Cmd).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
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
    /// A letter or digit typed: jump to the name starting with it.
    TypeAhead(char),
}

/// Explorer's keyboard.
#[must_use]
pub fn command_for(key: Key, mods: Mods) -> Option<Command> {
    let Mods { shift, ctrl, alt } = mods;
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
        Key::Char('n') if ctrl && shift && !alt => Command::NewFolder,
        Key::Char('r') if plain_ctrl => Command::Refresh,
        Key::Char('f' | 'e') if plain_ctrl => Command::Search,
        Key::Char('c') if plain_ctrl => Command::Copy,
        Key::Char('x') if plain_ctrl => Command::Cut,
        Key::Char('v') if plain_ctrl => Command::Paste,
        Key::Char('a') if plain_ctrl => Command::SelectAll,
        Key::Char('z') if plain_ctrl => Command::Undo,
        Key::Char(c) if !ctrl && !alt => Command::TypeAhead(c),
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
    };
    const CTRL: Mods = Mods {
        shift: false,
        ctrl: true,
        alt: false,
    };
    const SHIFT: Mods = Mods {
        shift: true,
        ctrl: false,
        alt: false,
    };
    const ALT: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: true,
    };
    const CTRL_SHIFT: Mods = Mods {
        shift: true,
        ctrl: true,
        alt: false,
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
