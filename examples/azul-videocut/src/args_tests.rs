use azul_appkit::args::{ModePref, Theme};

use super::*;

fn parse(args: &[&str]) -> Result<(AppArgs, Args), ParseError> {
    Args::parse(args.iter().copied())
}

#[test]
fn no_arguments_is_the_last_project_or_the_empty_state() {
    let (app, a) = parse(&[]).expect("no arguments");
    assert!(!a.sample && a.project.is_none());
    assert_eq!(a.screen, Screen::Editor);
    assert!(app.theme.is_none() && app.mode.is_none(), "the settings file decides");
}

#[test]
fn the_switches_map_by_name_in_both_spellings() {
    let (app, a) = parse(&["--sample", "--screen", "export", "--theme=flora", "--mode", "dark"]).expect("switches");
    assert!(a.sample);
    assert_eq!(a.screen, Screen::Export);
    assert_eq!(app.theme, Some(Theme::Flora));
    assert_eq!(app.mode, Some(ModePref::Dark));
    assert_eq!(parse(&["abc"]).expect("project").1.project.as_deref(), Some("abc"));
    assert_eq!(parse(&["--size=900x600"]).expect("size").0.size, Some((900.0, 600.0)));
    assert_eq!(parse(&["--screen", "settings"]).expect("s").1.screen, Screen::Settings);
    assert_eq!(parse(&["--screen", "about"]).expect("s").1.screen, Screen::About);
    let (app, _) = parse(&["--data-dir", "/tmp/vc"]).expect("data");
    assert_eq!(app.data_dir, Some(std::path::PathBuf::from("/tmp/vc")));
}

#[test]
fn a_bad_option_is_rejected_rather_than_ignored() {
    for bad in ["--theme=neon", "--mode=dim", "--screen=timeline-only", "--size=wide", "--nonsense"] {
        assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
    }
    assert!(parse(&["a", "b"]).is_err(), "one project at a time");
    assert!(parse(&["-h"]).unwrap_err().contains("USAGE"));
}
