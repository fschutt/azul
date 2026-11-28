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
