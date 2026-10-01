//! Explorer's keyboard as a table: which key with which modifiers runs which
//! command (Enter opens, Backspace / Alt+Up go up, Alt+Left / Alt+Right walk
//! the history, F2 renames, Delete / Shift+Delete, Ctrl+C / X / V, Ctrl+A,
//! Ctrl+Shift+N, F5, Ctrl+F / F3, the arrows with Shift / Ctrl, the context
//! menu key, type-ahead). Cmd counts as Ctrl on macOS. No azul types: the app
//! maps azul's key codes to [`Key`] and runs the [`Command`].

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
