use super::*;

/// A search and a command of the window run no program: the line keeps its status and drops the shell's
/// buttons while one of them is typed; a path, a shell command and an action are the shell's look.
#[test]
fn a_search_and_a_colon_command_are_the_windows_own_lines() {
    assert!(windows_own("?notes"));
    assert!(windows_own("  ?"));
    assert!(windows_own(":options"));
    assert!(windows_own(":"));
    assert!(!windows_own(""));
    assert!(!windows_own("ls -la"));
    assert!(!windows_own("cd Documents"));
    assert!(!windows_own("/help"));
    assert!(!windows_own("Home/Documents"));
}

/// A line that held a search goes back to its status when the search closed and nothing keeps it open; one
/// with a command typed in it, a program's output or a notice stays open.
#[test]
fn the_line_folds_when_a_search_closed_and_nothing_keeps_it_open() {
    let mut bar = CommandBar::default();
    bar.expanded = true;
    bar.input = String::from("  ");
    fold_if_idle(&mut bar);
    assert!(!bar.expanded, "empty and idle: back to the status");

    bar.expanded = true;
    bar.input = String::from("ls");
    fold_if_idle(&mut bar);
    assert!(bar.expanded, "a command being typed keeps it open");

    bar.input.clear();
    bar.notice = Some(Notice {
        text: Text::plain(String::from("a notice")),
        open_terminal: false,
    });
    fold_if_idle(&mut bar);
    assert!(bar.expanded, "a notice keeps it open");
}
