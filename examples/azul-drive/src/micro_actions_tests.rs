//! The command bar's "/" actions (`micro_actions`) without a window: /help lists the registry,
//! an unknown name says so, the git shortcuts run git in the shell, /switch opens the quick
//! switcher, and an action registered later is found with no change to the bar.

use std::path::Path;

use azul_appkit::l10n::Text;

use crate::micro_actions::{Context, MicroAction, Outcome, Registry};

fn cx() -> Context<'static> {
    Context {
        cwd: Path::new("/home/u/src/azul"),
        branch: Some("main"),
    }
}

/// The keys of the phrases of the lines an action said.
fn said_keys(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::Say(lines) => lines
            .iter()
            .flat_map(|line| line.keys().into_iter().map(str::to_string).collect::<Vec<_>>())
            .collect(),
        other => panic!("lines expected, got {other:?}"),
    }
}

#[test]
fn help_lists_every_action_with_its_summary() {
    let registry = Registry::with_builtins();
    let keys = said_keys(&registry.dispatch("help", "", &cx()));
    for name in registry.names() {
        let action = registry.find(name).expect("listed actions are found");
        assert!(
            keys.iter().any(|k| k == action.summary_key()),
            "/help names /{name}: {keys:?}"
        );
    }
    // A bare "/" is /help.
    assert_eq!(registry.dispatch("", "", &cx()), registry.dispatch("help", "", &cx()));
}

#[test]
fn an_unknown_action_says_so_and_points_to_help() {
    let registry = Registry::with_builtins();
    let outcome = registry.dispatch("frobnicate", "now", &cx());
    match &outcome {
        Outcome::Say(lines) => {
            let phrase = lines[0]
                .phrase("azdrive-cli-action-unknown")
                .expect("the unknown action's sentence");
            assert!(phrase.args.iter().any(|(n, v)| n == "name" && v.to_string() == "frobnicate"));
        }
        other => panic!("lines expected, got {other:?}"),
    }
}

#[test]
fn the_git_shortcuts_run_git_in_the_shell() {
    let registry = Registry::with_builtins();
    assert_eq!(
        registry.dispatch("status", "", &cx()),
        Outcome::Run(String::from("git status --short --branch"))
    );
    assert_eq!(
        registry.dispatch("log", "", &cx()),
        Outcome::Run(String::from("git log --oneline --graph --decorate -n 30"))
    );
    assert_eq!(
        registry.dispatch("git", "fetch --all", &cx()),
        Outcome::Run(String::from("git fetch --all"))
    );
    assert_eq!(
        registry.dispatch("git", "", &cx()),
        Outcome::Run(String::from("git status"))
    );
}

#[test]
fn switch_opens_the_quick_switcher_with_the_name_typed_and_the_rest_act_on_the_bar() {
    let registry = Registry::with_builtins();
    assert_eq!(
        registry.dispatch("switch", "feat", &cx()),
        Outcome::OpenSwitcher(String::from("feat"))
    );
    assert_eq!(registry.dispatch("clear", "", &cx()), Outcome::ClearOutput);
    assert_eq!(registry.dispatch("terminal", "", &cx()), Outcome::OpenTerminal);
}

/// An action of a later wave (a macro, an agent's micro-action): written against the trait
/// alone.
struct Echo;

impl MicroAction for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn summary_key(&self) -> &'static str {
        "azdrive-cli-action-help"
    }

    fn run(&self, args: &str, cx: &Context<'_>) -> Outcome {
        Outcome::Say(vec![Text::plain(format!("{args} in {}", cx.cwd.display()))])
    }
}

#[test]
fn an_action_registered_later_is_found_and_replaces_one_of_its_name() {
    let mut registry = Registry::with_builtins();
    let before = registry.names().len();
    registry.register(Box::new(Echo));
    assert_eq!(registry.names().len(), before + 1);
    assert_eq!(
        registry.dispatch("echo", "hi", &cx()),
        Outcome::Say(vec![Text::plain("hi in /home/u/src/azul")])
    );
    registry.register(Box::new(Echo));
    assert_eq!(registry.names().len(), before + 1, "one action per name");
}

#[test]
fn actions_complete_from_their_first_letters() {
    let registry = Registry::with_builtins();
    assert_eq!(registry.complete("st"), ["status"]);
    assert_eq!(registry.complete("sw"), ["switch"]);
    let mut all = registry.names();
    all.sort_unstable();
    assert_eq!(registry.complete(""), all);
    assert!(registry.complete("zz").is_empty());
}

#[test]
fn every_builtin_name_reads_as_an_action_when_typed() {
    let registry = Registry::with_builtins();
    for name in registry.names() {
        assert_eq!(
            crate::command_line::classify(&format!("/{name} x")),
            crate::command_line::Line::Action {
                name: name.to_string(),
                args: String::from("x")
            }
        );
    }
}
