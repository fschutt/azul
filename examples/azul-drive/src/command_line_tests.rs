//! The command bar's model (`command_line`) without a window: what a typed line is, where `cd`
//! goes, how the shell runs a line, which programs need a whole terminal, and the history.

use std::path::{Path, PathBuf};

use crate::command_line::{
    classify, invocation, needs_terminal, resolve_cd, CdTarget, History, Line, Platform,
};

fn shell(line: &str) -> Line {
    Line::Shell(line.to_string())
}

fn action(name: &str, args: &str) -> Line {
    Line::Action {
        name: name.to_string(),
        args: args.to_string(),
    }
}

fn cd(target: &str) -> Line {
    Line::ChangeFolder(target.to_string())
}

#[test]
fn a_line_is_a_shell_command_a_micro_action_or_a_folder_change() {
    assert_eq!(classify(""), Line::Empty);
    assert_eq!(classify("   "), Line::Empty);
    assert_eq!(classify("  git status  "), shell("git status"));
    assert_eq!(classify("/help"), action("help", ""));
    assert_eq!(classify("/switch feature/login "), action("switch", "feature/login"));
    assert_eq!(classify("/"), action("", ""));
    // A path to a program is a shell command, not an action.
    assert_eq!(classify("/usr/bin/env ls"), shell("/usr/bin/env ls"));
    assert_eq!(classify("/opt/tool"), shell("/opt/tool"));
    assert_eq!(classify("./build.sh"), shell("./build.sh"));
    assert_eq!(classify("cd .."), cd(".."));
    assert_eq!(classify("cd"), cd("~"));
    assert_eq!(classify("cd \"My Folder\""), cd("My Folder"));
    assert_eq!(classify("cd 'My Folder'"), cd("My Folder"));
    assert_eq!(classify("cd My\\ Folder"), cd("My Folder"));
    // More than a plain `cd`: the shell runs it (its `cd` stays in its own process).
    assert_eq!(classify("cd src && make"), shell("cd src && make"));
    assert_eq!(classify("cd a b"), shell("cd a b"));
    assert_eq!(classify("cd $HOME"), shell("cd $HOME"));
    assert_eq!(classify("cdk deploy"), shell("cdk deploy"));
}

#[test]
fn a_folder_change_resolves_against_the_open_folder_and_home() {
    let cwd = Path::new("/home/u/src");
    let home = Some(Path::new("/home/u"));
    let folder = |p: &str| Some(CdTarget::Folder(PathBuf::from(p)));
    assert_eq!(resolve_cd(cwd, "..", home), folder("/home/u"));
    assert_eq!(resolve_cd(cwd, "~", home), folder("/home/u"));
    assert_eq!(resolve_cd(cwd, "~/docs", home), folder("/home/u/docs"));
    assert_eq!(resolve_cd(cwd, "/tmp/x", home), folder("/tmp/x"));
    assert_eq!(resolve_cd(cwd, "a/./b/../c", home), folder("/home/u/src/a/c"));
    assert_eq!(resolve_cd(cwd, "../../..", home), folder("/"));
    assert_eq!(resolve_cd(cwd, "-", home), Some(CdTarget::Back));
    assert_eq!(resolve_cd(cwd, "~", None), None);
}

#[test]
fn the_shell_runs_the_line_as_a_login_shell_in_the_open_folder_with_the_pager_off() {
    let cwd = Path::new("/home/u/src");
    let run = invocation("git log", cwd, Some("/bin/zsh"), Platform::Unix);
    assert_eq!(run.program, "/bin/zsh");
    assert_eq!(run.args, ["-l", "-c", "git log"]);
    assert_eq!(run.cwd, cwd);
    for (name, value) in [
        ("GIT_PAGER", "cat"),
        ("PAGER", "cat"),
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
    ] {
        assert!(
            run.env.iter().any(|(n, v)| n == name && v == value),
            "{name}={value} in {:?}",
            run.env
        );
    }
    // No shell named: the POSIX one.
    assert_eq!(invocation("ls", cwd, None, Platform::Unix).program, "/bin/sh");
    assert_eq!(invocation("ls", cwd, Some(""), Platform::Unix).program, "/bin/sh");
    // csh takes `-l` only as its one flag.
    assert_eq!(
        invocation("ls", cwd, Some("/bin/tcsh"), Platform::Unix).args,
        ["-c", "ls"]
    );
    let win = invocation("dir", Path::new("C:\\Users\\u"), Some("C:\\Windows\\system32\\cmd.exe"), Platform::Windows);
    assert_eq!(win.program, "C:\\Windows\\system32\\cmd.exe");
    assert_eq!(win.args, ["/C", "dir"]);
    assert_eq!(invocation("dir", Path::new("C:\\"), None, Platform::Windows).program, "cmd.exe");
}

#[test]
fn full_screen_programs_are_named_before_they_run() {
    for (line, program) in [
        ("vim notes.txt", "vim"),
        ("nvim", "nvim"),
        ("sudo -u root nano /etc/hosts", "nano"),
        ("EDITOR=x less file", "less"),
        ("/usr/bin/top", "top"),
        ("cat big.log | less", "less"),
        ("make && htop", "htop"),
        ("man git", "man"),
        ("tmux new -s work", "tmux"),
        ("git commit", "git commit"),
        ("git commit --amend", "git commit"),
        ("git rebase -i HEAD~3", "git rebase -i"),
        ("git add -p", "git add -p"),
        ("ssh build-box", "ssh"),
        ("ssh -p 2222 build-box", "ssh"),
    ] {
        assert_eq!(needs_terminal(line).as_deref(), Some(program), "{line}");
    }
    for line in [
        "ls -la",
        "git status",
        "git log --oneline",
        "git commit -m 'fix'",
        "git commit --amend --no-edit",
        "git commit -F msg.txt",
        "git rebase main",
        "echo vim",
        "ssh build-box uptime",
        "ssh -p 2222 build-box ls -la",
        "cargo build",
    ] {
        assert_eq!(needs_terminal(line), None, "{line}");
    }
}

#[test]
fn the_history_walks_back_and_forth_and_keeps_what_was_being_typed() {
    let mut history = History::new(100);
    for line in ["git status", "ls", "make"] {
        history.push(line);
    }
    assert_eq!(history.up("git sw").as_deref(), Some("make"));
    assert_eq!(history.up("make").as_deref(), Some("ls"));
    assert_eq!(history.up("ls").as_deref(), Some("git status"));
    assert_eq!(history.up("git status"), None, "nothing older");
    assert_eq!(history.down().as_deref(), Some("ls"));
    assert_eq!(history.down().as_deref(), Some("make"));
    assert_eq!(history.down().as_deref(), Some("git sw"), "back to what was typed");
    assert_eq!(history.down(), None);
    // Running a line starts the walk anew from the newest.
    history.push("cargo test");
    assert_eq!(history.up("").as_deref(), Some("cargo test"));
}

#[test]
fn a_repeated_line_is_kept_once_and_the_oldest_leave_past_the_limit() {
    let mut history = History::new(3);
    for line in ["a", "b", "b", "c", "d"] {
        history.push(line);
    }
    assert_eq!(history.entries(), ["b", "c", "d"]);
}

#[test]
fn a_blank_line_or_one_starting_with_a_space_is_not_kept() {
    let mut history = History::new(10);
    history.push("");
    history.push("   ");
    history.push(" export TOKEN=secret");
    assert!(history.entries().is_empty());
}

#[test]
fn the_history_round_trips_through_its_file_keeping_the_newest() {
    let mut history = History::new(10);
    for line in ["git status", "ls -la", "make"] {
        history.push(line);
    }
    let text = history.to_text();
    assert_eq!(text, "git status\nls -la\nmake\n");
    let back = History::from_text(&text, 10);
    assert_eq!(back.entries(), history.entries());
    let trimmed = History::from_text("one\n\ntwo\nthree\n", 2);
    assert_eq!(trimmed.entries(), ["two", "three"]);
}
