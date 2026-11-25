use azul_css::env::{MapEnv, NoEnv};

use super::*;

fn with(pairs: &[(&str, &str)]) -> LinuxSettings {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    LinuxSettings::from_lookup(&env)
}

#[test]
fn the_empty_environment_is_the_default_and_traces_nothing() {
    let none = LinuxSettings::from_lookup(&NoEnv);
    assert_eq!(none, LinuxSettings::default());
    assert_eq!(
        none,
        LinuxSettings {
            wl_trace: false,
            bb_debug: false,
            bb_probe: None,
            present_verify: false,
            no_pace: false,
            x11_shm: true,
            disable_gnome_menus: false,
            gnome_menu_debug: false,
            a11y_debug: false,
        },
        "frame pacing, MIT-SHM and the GNOME menus are on by default"
    );
}

#[test]
fn a_trace_switch_is_on_when_the_variable_is_set_to_anything() {
    assert!(with(&[("AZ_WL_TRACE", "")]).wl_trace, "var_os(..).is_some()");
    assert!(with(&[("AZ_WL_TRACE", "0")]).wl_trace);
    assert!(with(&[("AZ_BB_DEBUG", "")]).bb_debug, "var(..).is_ok()");
    assert!(with(&[("AZ_BB_DEBUG", "no")]).bb_debug);
    assert!(with(&[("AZ_PRESENT_VERIFY", "")]).present_verify);
    assert!(with(&[("AZ_MAP_DEBUG", "1")]).a11y_debug);
    assert!(!with(&[("AZ_BB_DEBUG_X", "1")]).bb_debug);
    assert_eq!(with(&[("AZ_BB_PROBE", "3, 4")]).bb_probe.as_deref(), Some("3, 4"));
}

#[test]
fn no_pace_and_the_two_gnome_menu_switches_want_exactly_one() {
    assert!(with(&[("AZ_NO_PACE", "1")]).no_pace);
    for other in ["0", "", "true", "yes", "11"] {
        assert!(!with(&[("AZ_NO_PACE", other)]).no_pace, "{other:?}");
        assert!(
            !with(&[("AZ_DISABLE_GNOME_MENUS", other)]).disable_gnome_menus,
            "{other:?}"
        );
        assert!(!with(&[("AZ_GNOME_MENU_DEBUG", other)]).gnome_menu_debug, "{other:?}");
    }
    assert!(with(&[("AZ_DISABLE_GNOME_MENUS", "1")]).disable_gnome_menus);
    assert!(with(&[("AZ_GNOME_MENU_DEBUG", "1")]).gnome_menu_debug);
}

#[test]
fn x11_shm_is_off_only_for_a_zero() {
    assert!(!with(&[("AZ_X11_SHM", "0")]).x11_shm);
    for other in ["1", "", "off", "false", "00"] {
        assert!(with(&[("AZ_X11_SHM", other)]).x11_shm, "{other:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_value_that_is_not_utf8_counts_as_set_only_where_the_old_read_was_var_os() {
    use std::os::unix::ffi::OsStringExt;

    let raw = std::ffi::OsString::from_vec(vec![0xff, 0xfe]);
    let env = MapEnv::new()
        .with_os("AZ_WL_TRACE", raw.clone())
        .with_os("AZ_PRESENT_VERIFY", raw.clone())
        .with_os("AZ_BB_DEBUG", raw.clone())
        .with_os("AZ_X11_SHM", raw);
    let settings = LinuxSettings::from_lookup(&env);
    assert!(settings.wl_trace, "was var_os(..).is_some()");
    assert!(settings.present_verify, "was var_os(..).is_some()");
    assert!(!settings.bb_debug, "was var(..).is_ok(), an Err for a non-UTF-8 value");
    assert!(settings.x11_shm, "was var(..).map_or(true, ..), true for an Err");
}
