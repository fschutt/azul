use crate::command_line::{
    classify, colon_command, colon_complete, command_name, Colon, Line, COLON,
};

/// A line that starts with `?` is a search of the folder for what follows; a bare `?` is the
/// empty search (the search view with the recent searches).
#[test]
fn a_question_mark_starts_a_search() {
    assert_eq!(classify("?report"), Line::Search(String::from("report")));
    assert_eq!(classify("  ?  quarterly report  "), Line::Search(String::from("quarterly report")));
    assert_eq!(classify("?"), Line::Search(String::new()));
}

/// A line that starts with `:` is a command of the window, its name in one normal form.
#[test]
fn a_colon_starts_a_command_of_the_window() {
    assert_eq!(classify(":options"), Line::Command(String::from("options")));
    assert_eq!(classify(":New   Window"), Line::Command(String::from("new window")));
    assert_eq!(classify(":"), Line::Command(String::new()), "a bare colon lists the commands");
}

/// What did not start with them is what it was: a shell command (a `?` or `:` inside a line
/// is the shell's), a `/` action, a `cd`.
#[test]
fn the_other_lines_keep_their_meaning() {
    assert_eq!(classify("ls -la"), Line::Shell(String::from("ls -la")));
    assert_eq!(classify("echo a?b:c"), Line::Shell(String::from("echo a?b:c")));
    assert_eq!(
        classify("/help"),
        Line::Action {
            name: String::from("help"),
            args: String::new()
        }
    );
    assert_eq!(classify("cd /tmp"), Line::ChangeFolder(String::from("/tmp")));
    assert_eq!(classify("   "), Line::Empty);
}

/// The commands the task names: options, new window, about, sign in, exit.
#[test]
fn the_window_commands_are_found_by_their_names() {
    assert_eq!(colon_command("options"), Some(Colon::Options));
    assert_eq!(colon_command("new window"), Some(Colon::NewWindow));
    assert_eq!(colon_command("about"), Some(Colon::About));
    assert_eq!(colon_command("sign in"), Some(Colon::SignIn));
    assert_eq!(colon_command("exit"), Some(Colon::Exit));
    assert_eq!(colon_command("  Sign   IN "), Some(Colon::SignIn), "case and spaces do not matter");
    assert_eq!(colon_command("reboot"), None);
    assert_eq!(colon_command(""), None);
}

/// A name is in the table once, in its normal form (what `command_name` makes of it).
#[test]
fn the_names_are_unique_and_in_their_normal_form() {
    let mut seen = std::collections::HashSet::new();
    for (name, _) in COLON {
        assert_eq!(command_name(name), name, "{name} is not in its normal form");
        assert!(seen.insert(name), "{name} is there twice");
    }
}

/// Tab after a colon completes the names that start with what was typed.
#[test]
fn tab_completes_the_names_that_start_with_the_text() {
    assert_eq!(colon_complete("new"), ["new tab", "new window"]);
    assert_eq!(colon_complete("op"), ["options"]);
    assert_eq!(colon_complete("zzz"), Vec::<&str>::new());
    assert!(colon_complete("").len() >= 5, "nothing typed: every name");
}

/// A line holds a query when anything stands after the question mark: Escape empties a query
/// first and closes the view only when there is none.
#[test]
fn a_search_line_holds_a_query_when_something_follows_the_question_mark() {
    use crate::ui_command_bar::search_has_query;
    assert!(search_has_query("?notes"));
    assert!(search_has_query(" ? notes "));
    assert!(!search_has_query("?"));
    assert!(!search_has_query("?   "));
    assert!(!search_has_query("notes"), "no question mark: no search");
    assert!(!search_has_query(""));
}
