//! The command bar's model, without azul types: what a typed line is (a shell command, a "/"
//! micro-action, a `cd`), how the shell runs a line (the user's login shell in the open folder,
//! the pagers off, a terminal type the bar's screen understands), which programs need a whole
//! terminal, and the history of the lines run (up / down; kept per profile in the data tree by
//! `ui_command_bar`).
//!
//! The bar runs one line at a time as `<shell> -l -c <line>`: the user's own shell with the
//! user's rights and profile (a login shell reads the PATH a GUI app does not get on macOS),
//! nothing hidden and nothing elevated. A `cd` alone would change only that shell's folder and
//! end with it, so the bar takes it as a navigation of the folder view instead.

use std::path::{Component, Path, PathBuf};

/// What a typed line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Nothing typed.
    Empty,
    /// `/name args`: a micro-action of the registry (`micro_actions`). A bare `/` is `/help`
    /// (an empty name).
    Action { name: String, args: String },
    /// `cd <folder>` alone: the folder view goes there (`~` for a bare `cd`).
    ChangeFolder(String),
    /// Anything else: the shell runs it.
    Shell(String),
}

/// Whether `word` is a micro-action's name (`help`, `git`): a lower-case letter, then letters,
/// digits and dashes. `/usr/bin/env` is no name, so a program's path stays a shell command.
fn is_action_name(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// What `text` asks for.
#[must_use]
pub fn classify(text: &str) -> Line {
    let line = text.trim();
    if line.is_empty() {
        return Line::Empty;
    }
    if let Some(rest) = line.strip_prefix('/') {
        let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if name.is_empty() || is_action_name(name) {
            return Line::Action {
                name: name.to_string(),
                args: args.trim().to_string(),
            };
        }
        return Line::Shell(line.to_string());
    }
    if line == "cd" {
        return Line::ChangeFolder(String::from("~"));
    }
    if let Some(arg) = line.strip_prefix("cd ") {
        if let Some(folder) = plain_argument(arg.trim()) {
            return Line::ChangeFolder(folder);
        }
    }
    Line::Shell(line.to_string())
}

/// `arg` as ONE plain folder name: unquoted from matching quotes, `\ ` read as a space; `None`
/// for anything the shell itself would have to read (several words, variables, operators).
fn plain_argument(arg: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(inner) = arg
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return (!inner.contains(quote) && !inner.is_empty()).then(|| inner.to_string());
        }
    }
    let unescaped = arg.replace("\\ ", "\u{0}");
    let special = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '$' | '`' | '"' | '\'' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '*' | '?' | '\\'
            )
    };
    if unescaped.is_empty() || unescaped.chars().any(special) {
        return None;
    }
    Some(unescaped.replace('\u{0}', " "))
}

/// Where a `cd` goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdTarget {
    /// A folder on this computer.
    Folder(PathBuf),
    /// `cd -`: the folder before (the view's Back).
    Back,
}

/// Where `cd <arg>` from `cwd` goes: `~` and `~/x` from `home`, an absolute path as it is,
/// anything else from `cwd`, with `.` and `..` resolved by name (as the shell's `cd` does).
/// `None` for `~` without a home folder.
#[must_use]
pub fn resolve_cd(cwd: &Path, arg: &str, home: Option<&Path>) -> Option<CdTarget> {
    if arg == "-" {
        return Some(CdTarget::Back);
    }
    let joined = if arg == "~" {
        home?.to_path_buf()
    } else if let Some(rest) = arg.strip_prefix("~/") {
        home?.join(rest)
    } else {
        cwd.join(arg)
    };
    let mut out = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never above the root (`/..` is `/`).
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(CdTarget::Folder(out))
}

/// The system the shell runs on: how it is called differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Unix,
    Windows,
}

impl Platform {
    /// The system this build runs on.
    #[must_use]
    pub const fn this() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Unix
        }
    }
}

/// How the shell runs one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    /// The folder the view shows.
    pub cwd: PathBuf,
    /// Added to the user's environment (the pagers off, the terminal type).
    pub env: Vec<(String, String)>,
}

/// The variables a line runs with, beyond the user's: no pager waits for a key (the bar's
/// screen is not a terminal to page in), and programs colour for the bar's VT screen (an app
/// started from the Dock or Finder has no `TERM`).
const ENV: &[(&str, &str)] = &[
    ("GIT_PAGER", "cat"),
    ("PAGER", "cat"),
    ("TERM", "xterm-256color"),
    ("COLORTERM", "truecolor"),
    ("CLICOLOR", "1"),
];

/// How `shell` (the user's: `$SHELL`, `%COMSPEC%`; `None` or empty for the system's) runs
/// `line` in `cwd` on `platform`: `<shell> -l -c <line>` (csh and tcsh take `-l` only alone, so
/// they get `-c`), `cmd.exe /C <line>` on Windows.
#[must_use]
pub fn invocation(line: &str, cwd: &Path, shell: Option<&str>, platform: Platform) -> Invocation {
    let shell = shell.map(str::trim).filter(|s| !s.is_empty());
    let (program, args) = match platform {
        Platform::Unix => {
            let program = shell.unwrap_or("/bin/sh").to_string();
            let name = Path::new(&program)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let args = if matches!(name.as_str(), "csh" | "tcsh") {
                vec![String::from("-c"), line.to_string()]
            } else {
                vec![String::from("-l"), String::from("-c"), line.to_string()]
            };
            (program, args)
        }
        Platform::Windows => (
            shell.unwrap_or("cmd.exe").to_string(),
            vec![String::from("/C"), line.to_string()],
        ),
    };
    Invocation {
        program,
        args,
        cwd: cwd.to_path_buf(),
        env: ENV
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
    }
}

/// `word` as ONE argument of the shell `platform` runs: in single quotes on Unix (a quote
/// inside written `'\''`), in double quotes for cmd.exe (a double quote doubled). A word of
/// plain characters (letters, digits, `-_./+@%=:,`) stays as it is.
#[must_use]
pub fn shell_quote(word: &str, platform: Platform) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./+@%=:,".contains(c));
    if plain {
        return word.to_string();
    }
    match platform {
        Platform::Unix => format!("'{}'", word.replace('\'', "'\\''")),
        Platform::Windows => format!("\"{}\"", word.replace('"', "\"\"")),
    }
}

/// Programs that take the whole terminal (an editor, a pager, a monitor, a multiplexer, a file
/// manager): the bar's one-line screen cannot host them.
const FULL_SCREEN: &[&str] = &[
    "vi", "vim", "nvim", "view", "vimdiff", "nano", "pico", "micro", "emacs", "less", "more",
    "most", "man", "top", "htop", "btop", "atop", "tmux", "screen", "mc", "ranger", "nnn", "lf",
    "vifm", "tig", "lazygit", "watch", "ncdu", "k9s",
];

/// Words that start another program after them (`sudo vim`), with their options that take a
/// value (`sudo -u root vim`): the program is what follows.
const LAUNCHERS: &[(&str, &str)] = &[
    ("sudo", "ugCDhprtU"),
    ("doas", "uC"),
    ("env", "uCS"),
    ("nice", "n"),
    ("command", ""),
    ("exec", ""),
    ("nohup", ""),
    ("time", ""),
];

/// ssh's options that take a value (`-p 22`): its value is no host.
const SSH_VALUE_OPTIONS: &str = "BbcDEeFIiJLlmOopQRSWw";

/// The program of `line` that needs a whole terminal, as the bar names it (`vim`, `git
/// commit`, `ssh`), or `None` when every program of the line is fine in the bar. Every command
/// of a pipeline or a list (`|`, `&&`, `||`, `;`) is looked at.
#[must_use]
pub fn needs_terminal(line: &str) -> Option<String> {
    line.split(|c| c == '|' || c == ';' || c == '&')
        .find_map(|part| command_needing_terminal(part.split_whitespace().collect()))
}

/// [`needs_terminal`] of one simple command's words.
fn command_needing_terminal(words: Vec<&str>) -> Option<String> {
    let mut rest = words.as_slice();
    // Variables set for the command (`EDITOR=x less file`) and launchers with their options.
    loop {
        let Some(word) = rest.first() else {
            return None;
        };
        if word.contains('=') && !word.starts_with('-') {
            rest = &rest[1..];
            continue;
        }
        let Some((_, takes_value)) = LAUNCHERS.iter().find(|(name, _)| *name == program_name(word))
        else {
            break;
        };
        rest = &rest[1..];
        while let Some(option) = rest.first().filter(|w| w.starts_with('-')) {
            rest = &rest[1..];
            // `-u root`: the value follows a one-letter option that takes one.
            let letter = option.strip_prefix('-').filter(|l| l.len() == 1);
            if letter.is_some_and(|l| takes_value.contains(l)) && !rest.is_empty() {
                rest = &rest[1..];
            }
        }
    }
    let (first, args) = rest.split_first()?;
    let program = program_name(first);
    if FULL_SCREEN.contains(&program) {
        if program == "emacs" && args.iter().any(|a| *a == "--batch" || *a == "-batch") {
            return None;
        }
        return Some(program.to_string());
    }
    match program {
        "git" => git_needing_terminal(args),
        "ssh" => {
            let mut words = 0;
            let mut i = 0;
            while i < args.len() {
                let arg = args[i];
                if let Some(flags) = arg.strip_prefix('-') {
                    // `-p 22`: the value follows; `-p22` holds it.
                    if flags.len() == 1 && SSH_VALUE_OPTIONS.contains(flags) {
                        i += 1;
                    }
                } else {
                    words += 1;
                }
                i += 1;
            }
            // `ssh host` alone opens a shell on it; `ssh host cmd` runs one command.
            (words <= 1).then(|| String::from("ssh"))
        }
        _ => None,
    }
}

/// A git command that opens an editor or asks interactively.
fn git_needing_terminal(args: &[&str]) -> Option<String> {
    let (sub, rest) = args.split_first()?;
    let has = |options: &[&str]| {
        rest.iter().any(|a| {
            options
                .iter()
                .any(|o| a == o || (o.starts_with("--") && a.starts_with(&format!("{o}="))))
        })
    };
    match *sub {
        "commit"
            if !has(&[
                "-m", "--message", "-F", "--file", "-C", "--reuse-message", "--no-edit",
            ]) && !rest.iter().any(|a| a.starts_with("-m") && a.len() > 2) =>
        {
            Some(String::from("git commit"))
        }
        "rebase" if has(&["-i", "--interactive"]) => Some(String::from("git rebase -i")),
        "add" if has(&["-p", "--patch"]) => Some(String::from("git add -p")),
        "add" if has(&["-i", "--interactive"]) => Some(String::from("git add -i")),
        "mergetool" | "difftool" => Some(format!("git {sub}")),
        _ => None,
    }
}

/// A program's name from how it is typed (`/usr/bin/top` is `top`).
#[must_use]
pub fn program_name(word: &str) -> &str {
    word.rsplit(['/', '\\']).next().unwrap_or(word)
}

/// The lines run, oldest first, and a walk through them with up / down that keeps the line
/// being typed when the walk began.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    entries: Vec<String>,
    /// The most lines kept; the oldest leave first.
    limit: usize,
    /// Where the up / down walk is (an index into `entries`); `None` when not walking.
    cursor: Option<usize>,
    /// What was typed when the walk began: down past the newest gives it back.
    draft: String,
}

/// The lines a history keeps unless told otherwise.
pub const HISTORY_LIMIT: usize = 500;

impl History {
    /// An empty history of at most `limit` lines.
    #[must_use]
    pub fn new(limit: usize) -> History {
        History {
            entries: Vec::new(),
            limit: limit.max(1),
            cursor: None,
            draft: String::new(),
        }
    }

    /// The history kept in `text` (one line each, oldest first), its newest `limit` lines.
    #[must_use]
    pub fn from_text(text: &str, limit: usize) -> History {
        let mut history = History::new(limit);
        for line in text.lines() {
            history.push(line);
        }
        history
    }

    /// The file's text: one line each, oldest first.
    #[must_use]
    pub fn to_text(&self) -> String {
        self.entries.iter().map(|line| format!("{line}\n")).collect()
    }

    /// The lines, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// `line` was run: it becomes the newest (once, not again right after itself) and the walk
    /// starts anew. A blank line is not kept, nor one starting with a space - the shells' habit
    /// for a line with a secret in it.
    pub fn push(&mut self, line: &str) {
        self.cursor = None;
        self.draft.clear();
        if line.trim().is_empty() || line.starts_with(' ') {
            return;
        }
        let line = line.replace(['\n', '\r'], " ");
        if self.entries.last() == Some(&line) {
            return;
        }
        self.entries.push(line);
        let over = self.entries.len().saturating_sub(self.limit);
        self.entries.drain(..over);
    }

    /// Up: the line before the one shown (the newest at first; `typed` is kept as the draft).
    /// `None` when there is nothing older.
    pub fn up(&mut self, typed: &str) -> Option<String> {
        let next = match self.cursor {
            None => {
                self.draft = typed.to_string();
                self.entries.len().checked_sub(1)?
            }
            Some(0) => return None,
            Some(at) => at - 1,
        };
        self.cursor = Some(next);
        self.entries.get(next).cloned()
    }

    /// Down: the line after the one shown, and past the newest what was typed before the walk.
    /// `None` when not walking.
    pub fn down(&mut self) -> Option<String> {
        let at = self.cursor?;
        if at + 1 < self.entries.len() {
            self.cursor = Some(at + 1);
            return self.entries.get(at + 1).cloned();
        }
        self.cursor = None;
        Some(std::mem::take(&mut self.draft))
    }

    /// The line was edited: the next up starts from the newest again.
    pub fn reset_walk(&mut self) {
        self.cursor = None;
    }
}
