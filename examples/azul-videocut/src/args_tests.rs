use super::*;

fn parse(args: &[&str]) -> Result<Args, ParseError> {
    Args::parse(args.iter().copied())
}

#[test]
fn no_arguments_is_the_last_project_or_the_empty_state() {
    let a = parse(&[]).expect("no arguments");
    assert!(!a.sample && a.project.is_none());
    assert_eq!(a.screen, Screen::Editor);
    assert!(a.theme.is_none() && a.mode.is_none());
}

#[test]
fn the_switches_map_by_name_in_both_spellings() {
    let a = parse(&["--sample", "--screen", "export", "--theme=flora", "--mode", "dark"])
        .expect("switches");
    assert!(a.sample);
    assert_eq!(a.screen, Screen::Export);
    assert_eq!(a.theme.as_deref(), Some("flora"));
    assert_eq!(a.mode, Some(Mode::Dark));
    assert_eq!(parse(&["--project", "abc"]).expect("project").project.as_deref(), Some("abc"));
    assert_eq!(parse(&["--size=900x600"]).expect("size").size, Some((900.0, 600.0)));
    assert_eq!(parse(&["--screen", "settings"]).expect("s").screen, Screen::Settings);
    assert_eq!(parse(&["--screen", "about"]).expect("s").screen, Screen::About);
}

#[test]
fn a_bad_option_is_rejected_rather_than_ignored() {
    for bad in [
        "--theme=neon",
        "--mode=dim",
        "--screen=timeline-only",
        "--size=wide",
        "--nonsense",
        "--project",
    ] {
        assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
    }
    assert!(parse(&["-h"]).unwrap_err().contains("USAGE"));
}
