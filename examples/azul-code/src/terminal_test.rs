use super::*;

fn replayed(number: u64) -> TerminalTab {
    TerminalTab {
        session: Session::replay(b"$ ", START_GRID, 100),
        title: format!("sh {number}"),
        number,
        exited: false,
        spoke: false,
    }
}

#[test]
fn removing_a_shell_keeps_the_one_in_front_or_takes_its_neighbour() {
    let mut p = Panel::default();
    for n in 1..=3 {
        p.terminals.push(replayed(n));
    }
    p.active = 2;
    p.remove(0);
    assert_eq!(p.terminals[p.active].number, 3, "the shell in front stays in front");
    p.remove(p.active);
    assert_eq!(p.terminals[p.active].number, 2, "the last one closed: the one before it");
    p.remove(0);
    assert!(p.terminals.is_empty());
    assert_eq!(p.active, 0);
    p.remove(5);
    assert!(p.active_mut().is_none());
}

#[test]
fn a_new_shell_starts_in_the_workspace_or_at_home() {
    let folder = PathBuf::from("/work/project");
    assert_eq!(shell_folder(Some(folder.clone())), Some(folder));
    assert_eq!(shell_folder(None), pane::home_dir());
}
