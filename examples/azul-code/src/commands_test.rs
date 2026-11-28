use super::*;

fn mods(primary: bool, shift: bool, ctrl: bool) -> KeyModifiers {
    let mac = cfg!(target_os = "macos");
    KeyModifiers {
        shift,
        ctrl: ctrl || (primary && !mac),
        alt: false,
        meta: primary && mac,
    }
}

#[test]
fn the_window_keys_are_vscodes() {
    use VirtualKeyCode as K;
    let m = |p, s, c| mods(p, s, c);
    assert_eq!(command_of(K::P, m(true, false, false), false), Some(Action::QuickOpen));
    assert_eq!(command_of(K::P, m(true, true, false), false), Some(Action::CommandPalette));
    assert_eq!(command_of(K::F, m(true, true, false), false), Some(Action::FindInFiles));
    assert_eq!(command_of(K::F, m(true, false, false), false), None, "no file: no find bar");
    assert_eq!(command_of(K::F, m(true, false, false), true), Some(Action::Find));
    assert_eq!(command_of(K::Grave, m(false, false, true), false), Some(Action::ToggleTerminal));
    assert_eq!(command_of(K::Grave, m(false, true, true), false), Some(Action::NewTerminal));
    assert_eq!(command_of(K::J, m(true, false, false), false), Some(Action::ToggleTerminal));
    assert_eq!(command_of(K::O, m(true, false, false), false), Some(Action::OpenFolder));
    assert_eq!(command_of(K::A, m(false, false, false), true), None, "typing is the editor's");
    // the views of the card: source control, run, the panel's problems and output (VSCode's letters, Shift added)
    assert_eq!(command_of(K::G, m(true, true, false), false), Some(Action::ShowSourceControl));
    assert_eq!(command_of(K::G, m(true, false, false), true), Some(Action::GoToLine), "without Shift: the line");
    assert_eq!(command_of(K::D, m(true, true, false), false), Some(Action::ShowRun));
    assert_eq!(command_of(K::M, m(true, true, false), false), Some(Action::ShowProblems));
    assert_eq!(command_of(K::U, m(true, true, false), false), Some(Action::ShowOutput));
    assert_eq!(command_of(K::D, m(true, false, false), false), None, "Mod+D is the editor's (the next occurrence)");
    assert_eq!(chord_command(K::O, m(true, false, false)), Some(Action::OpenFolder));
    assert_eq!(chord_command(K::F, m(false, false, false)), Some(Action::CloseFolder));
    assert_eq!(chord_command(K::X, m(true, false, false)), None);
    assert!(modifier_alone(K::LWin) && modifier_alone(K::RControl) && !modifier_alone(K::K));
}

#[test]
fn a_picked_folders_trailing_slash_is_left_out() {
    let root = folder_root(Path::new("/home/me/project/"));
    assert_eq!(root.drive_root, PathBuf::from("/home/me/project"));
    assert_eq!(root.name, "project");
    assert_eq!(root.drive_root.display().to_string(), "/home/me/project");
    assert_eq!(
        keys("Mod+K Mod+O"),
        if cfg!(target_os = "macos") { "\u{2318}K \u{2318}O" } else { "Ctrl+K Ctrl+O" }
    );
    assert_eq!(
        keys("F3 / Shift+F3"),
        if cfg!(target_os = "macos") { "F3 / \u{21e7}F3" } else { "F3 / Shift+F3" }
    );
}

#[test]
fn a_view_shown_by_a_command_opens_the_drawer_on_a_phone_and_the_side_bar_on_a_desktop() {
    let mut st = AppState::new(RefAny::new(0u8), std::env::temp_dir(), false);
    st.window = (1280.0, 800.0);
    st.side_visible = false;
    show_side(&mut st, Side::Git);
    assert_eq!((st.side, st.side_visible, st.phone.drawer), (Side::Git, true, false), "a desktop has the side bar");
    st.window = (411.0, 914.0);
    st.phone.editing = true;
    show_side(&mut st, Side::Run);
    assert_eq!(st.side, Side::Run);
    assert!(st.phone.drawer, "a phone has the drawer");
    assert!(!st.phone.editing, "the drawer leaves the editor: no key row over it");
}
