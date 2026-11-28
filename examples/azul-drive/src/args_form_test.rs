use crate::args::{new_window_args, usage, Args, FormArg, FORM_VAR};

fn parse(args: &[&str]) -> Result<Args, String> {
    Args::parse(args.iter().map(|s| s.to_string()))
}

/// `--form` names the layout the window is built for, after a space or an equals sign; without
/// it the window decides.
#[test]
fn the_form_switch_names_the_layout() {
    assert_eq!(parse(&[]).unwrap().form, None);
    assert_eq!(parse(&["--form", "phone"]).unwrap().form, Some(FormArg::Phone));
    assert_eq!(parse(&["--form=desktop"]).unwrap().form, Some(FormArg::Desktop));
    assert_eq!(parse(&["--form", "auto"]).unwrap().form, Some(FormArg::Auto));
}

/// A name that is none of the three is refused with what was expected, and the switch needs a
/// value.
#[test]
fn a_form_that_is_no_form_is_refused() {
    let error = parse(&["--form", "tablet"]).unwrap_err();
    assert!(error.contains("--form"), "{error}");
    assert!(error.contains("tablet"), "{error}");
    assert!(parse(&["--form"]).is_err());
}

/// The environment fills in a switch that was not given (older scripts), and a given switch wins.
#[test]
fn the_environment_fills_in_the_form_and_the_switch_wins() {
    let env = |name: &str| (name == FORM_VAR).then(|| String::from("phone"));
    assert_eq!(parse(&[]).unwrap().with_env_fallbacks(env).form, Some(FormArg::Phone));
    assert_eq!(
        parse(&["--form", "desktop"]).unwrap().with_env_fallbacks(env).form,
        Some(FormArg::Desktop)
    );
    let nonsense = |name: &str| (name == FORM_VAR).then(|| String::from("sideways"));
    assert_eq!(parse(&[]).unwrap().with_env_fallbacks(nonsense).form, None);
}

/// Another window of the run (File > New window) is built for the same layout, and the usage
/// text names the switch.
#[test]
fn a_new_window_keeps_the_form_and_the_usage_names_it() {
    let kept = new_window_args(["--form", "phone"], "Home/Documents");
    assert_eq!(kept, ["--form", "phone", "--open", "Home/Documents"]);
    assert!(usage().contains("--form"));
}
