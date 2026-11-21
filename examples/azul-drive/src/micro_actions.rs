//! The command bar's micro-actions: a line starting with "/" runs a named action of a registry
//! instead of a shell command (`/help`, `/status`, `/switch feat`). No azul types.
//!
//! An action is anything that implements [`MicroAction`]: a name, the key of its one-line
//! summary (for /help), and a `run` that turns its arguments into an [`Outcome`]. The bar
//! carries the outcome out (prints lines, runs a shell line, opens the quick switcher), so a
//! later macro or agent action is one more `register` call, with no change to the bar. The
//! built-in ones are small shortcuts around git and the bar itself.

use std::path::Path;

use azul_appkit::l10n::{Phrase, Text};

/// What an action sees of the window: the folder the bar runs in and its branch.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The folder the view shows (the shell's working folder).
    pub cwd: &'a Path,
    /// The branch of the repository holding it, when there is one.
    pub branch: Option<&'a str>,
}

/// What the bar does after an action ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Lines for the output pane.
    Say(Vec<Text>),
    /// A line for the shell, run as if typed (same folder, same rules).
    Run(String),
    /// The quick switcher opens with this filter typed.
    OpenSwitcher(String),
    /// The output pane empties.
    ClearOutput,
    /// The system's terminal opens in the folder.
    OpenTerminal,
}

/// One "/" action.
pub trait MicroAction {
    /// What follows the "/" (`status`): a lower-case letter, then letters, digits and dashes.
    fn name(&self) -> &'static str;
    /// The key of its one-line summary, which /help lists.
    fn summary_key(&self) -> &'static str;
    /// Its arguments as /help shows them (`<branch>`); empty without.
    fn usage(&self) -> &'static str {
        ""
    }
    /// What it does with `args` (the rest of the line, trimmed).
    fn run(&self, args: &str, cx: &Context<'_>) -> Outcome;
}

/// The actions the bar knows, one per name.
#[derive(Default)]
pub struct Registry {
    actions: Vec<Box<dyn MicroAction>>,
}

impl Registry {
    /// The built-in actions: /help, /status, /log, /git, /switch, /clear, /terminal.
    #[must_use]
    pub fn with_builtins() -> Registry {
        let mut registry = Registry::default();
        registry.register(Box::new(Help));
        registry.register(Box::new(Shortcut {
            name: "status",
            summary_key: "azdrive-cli-action-status",
            line: "git status --short --branch",
        }));
        registry.register(Box::new(Shortcut {
            name: "log",
            summary_key: "azdrive-cli-action-log",
            line: "git log --oneline --graph --decorate -n 30",
        }));
        registry.register(Box::new(Git));
        registry.register(Box::new(Switch));
        registry.register(Box::new(Clear));
        registry.register(Box::new(Terminal));
        registry
    }

    /// Adds `action`; one of the same name is replaced.
    pub fn register(&mut self, action: Box<dyn MicroAction>) {
        self.actions.retain(|a| a.name() != action.name());
        self.actions.push(action);
    }

    /// The action named `name`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&dyn MicroAction> {
        self.actions
            .iter()
            .find(|a| a.name() == name)
            .map(|a| a.as_ref())
    }

    /// Every action's name, in the order they were registered.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.actions.iter().map(|a| a.name()).collect()
    }

    /// The names starting with `prefix`, sorted: what Tab completes after a "/".
    #[must_use]
    pub fn complete(&self, prefix: &str) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self
            .names()
            .into_iter()
            .filter(|n| n.starts_with(prefix))
            .collect();
        names.sort_unstable();
        names
    }

    /// Runs `/name args`. An empty name is /help; an unknown one says so.
    #[must_use]
    pub fn dispatch(&self, name: &str, args: &str, cx: &Context<'_>) -> Outcome {
        if name.is_empty() || name == "help" {
            return Outcome::Say(self.help_lines());
        }
        match self.find(name) {
            Some(action) => action.run(args, cx),
            None => Outcome::Say(vec![
                Phrase::new("azdrive-cli-action-unknown").arg("name", name).into(),
            ]),
        }
    }

    /// /help: a line of what the bar does, then one per action - its name and usage, then its
    /// summary.
    fn help_lines(&self) -> Vec<Text> {
        let mut lines = vec![Text::key("azdrive-cli-help-intro")];
        let mut actions: Vec<&dyn MicroAction> = self.actions.iter().map(|a| a.as_ref()).collect();
        actions.sort_by_key(|a| a.name());
        for action in actions {
            let call = if action.usage().is_empty() {
                format!("/{}", action.name())
            } else {
                format!("/{} {}", action.name(), action.usage())
            };
            lines.push(
                Text::plain(format!("  {call:<18}")).then(Phrase::new(action.summary_key())),
            );
        }
        lines
    }
}

/// /help (the registry answers it itself: it lists the registry).
struct Help;

impl MicroAction for Help {
    fn name(&self) -> &'static str {
        "help"
    }

    fn summary_key(&self) -> &'static str {
        "azdrive-cli-action-help"
    }

    fn run(&self, _args: &str, _cx: &Context<'_>) -> Outcome {
        // `Registry::dispatch` lists the registry before an action is looked up.
        Outcome::Say(vec![Text::key("azdrive-cli-help-intro")])
    }
}

/// A fixed shell line under a short name (`/status`).
struct Shortcut {
    name: &'static str,
    summary_key: &'static str,
    line: &'static str,
}

impl MicroAction for Shortcut {
    fn name(&self) -> &'static str {
        self.name
    }

    fn summary_key(&self) -> &'static str {
        self.summary_key
    }

    fn run(&self, _args: &str, _cx: &Context<'_>) -> Outcome {
        Outcome::Run(self.line.to_string())
    }
}

/// `/git <args>`: git with the pager off, `git status` without arguments.
struct Git;

impl MicroAction for Git {
    fn name(&self) -> &'static str {
        "git"
    }

    fn summary_key(&self) -> &'static str {
        "azdrive-cli-action-git"
    }

    fn usage(&self) -> &'static str {
        "<args>"
    }

    fn run(&self, args: &str, _cx: &Context<'_>) -> Outcome {
        if args.is_empty() {
            Outcome::Run(String::from("git status"))
        } else {
            Outcome::Run(format!("git {args}"))
        }
    }
}

/// `/switch [filter]`: the quick switcher, its filter typed.
struct Switch;

impl MicroAction for Switch {
    fn name(&self) -> &'static str {
        "switch"
    }

    fn summary_key(&self) -> &'static str {
        "azdrive-cli-action-switch"
    }

    fn usage(&self) -> &'static str {
        "[branch]"
    }

    fn run(&self, args: &str, _cx: &Context<'_>) -> Outcome {
        Outcome::OpenSwitcher(args.to_string())
    }
}

/// `/clear`: the output pane empties.
struct Clear;

impl MicroAction for Clear {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn summary_key(&self) -> &'static str {
        "azdrive-cli-action-clear"
    }

    fn run(&self, _args: &str, _cx: &Context<'_>) -> Outcome {
        Outcome::ClearOutput
    }
}

/// `/terminal`: the system's terminal in the folder (for what needs a whole one).
struct Terminal;

impl MicroAction for Terminal {
    fn name(&self) -> &'static str {
        "terminal"
    }

    fn summary_key(&self) -> &'static str {
        "azdrive-cli-action-terminal"
    }

    fn run(&self, _args: &str, _cx: &Context<'_>) -> Outcome {
        Outcome::OpenTerminal
    }
}
