use azul_css::env::{EnvSource, MapEnv, NoEnv};

use super::*;

/// Every variable of the snapshot, so the tests below cannot forget one.
const NAMES: [&str; 29] = [
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "WAYLAND_DISPLAY_WIDTH",
    "WAYLAND_DISPLAY_HEIGHT",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
    "DESKTOP_SESSION",
    "GNOME_DESKTOP_SESSION_ID",
    "KDE_FULL_SESSION",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "SWAYSOCK",
    "I3SOCK",
    "DBUS_SESSION_BUS_ADDRESS",
    "FLATPAK_ID",
    "LANGUAGE",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "XCURSOR_THEME",
    "XCURSOR_SIZE",
    "HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "XDG_CONFIG_DIRS",
    "GTK_CONFIG_HOME",
    "PATH",
    "LOCALAPPDATA",
    "USERPROFILE",
];

/// The map with every name of [`NAMES`] and `TEMP` set to a value of its own.
fn everything() -> MapEnv {
    let mut env = MapEnv::new();
    for name in NAMES {
        env.set(name, &format!("value of {name}"));
    }
    env.set("TEMP", "value of TEMP");
    env
}

#[test]
fn the_default_is_the_empty_environment() {
    assert_eq!(PlatformEnv::default(), PlatformEnv::from_lookup(&NoEnv));
    assert_eq!(PlatformEnv::default().var("HOME"), None);
    assert!(!PlatformEnv::default().is_set("DISPLAY"));
}

#[test]
fn every_variable_is_read_into_a_field_and_answered_back() {
    let snapshot = PlatformEnv::from_lookup(&everything());
    // no field is left unset: a name missing from `from_lookup` would show as `None`
    assert!(
        !format!("{snapshot:?}").contains("None"),
        "a field was not read: {snapshot:?}"
    );
    // the snapshot is a source over exactly the names it read ...
    for name in NAMES.iter().copied().chain(["TEMP"]) {
        let expected = format!("value of {name}");
        assert_eq!(snapshot.var(name), Some(expected.clone()), "{name}");
        assert_eq!(
            snapshot.var_os(name),
            Some(std::ffi::OsString::from(expected)),
            "{name} as an OS string"
        );
        assert!(snapshot.is_set(name), "{name}");
    }
    // ... so reading it again changes nothing
    assert_eq!(PlatformEnv::from_lookup(&snapshot), snapshot);
}

#[test]
fn the_snapshot_answers_nothing_it_did_not_read() {
    let snapshot = PlatformEnv::from_lookup(&everything().with("AZ_BACKEND", "headless"));
    assert_eq!(snapshot.var("AZ_BACKEND"), None);
    assert!(!snapshot.is_set("AZ_BACKEND"));
    assert_eq!(snapshot.var("XDG_RUNTIME_DIR"), None);
}

#[test]
fn an_empty_value_is_a_value_and_an_unset_one_is_none() {
    let env = PlatformEnv::from_lookup(&MapEnv::new().with("DISPLAY", "").with("HOME", ""));
    assert_eq!(env.display.as_deref(), Some(""));
    assert_eq!(env.home(), Some(""));
    assert_eq!(env.wayland_display, None);
    assert_eq!(env.xdg_data_home(), None);
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_utf8_survives_in_the_field_but_not_in_the_text_view() {
    use std::os::unix::ffi::OsStringExt;

    let raw = std::ffi::OsString::from_vec(vec![b'/', b'h', 0xff, b'm']);
    let env = PlatformEnv::from_lookup(&MapEnv::new().with_os("HOME", raw.clone()));
    assert_eq!(env.home, Some(raw.clone()));
    assert_eq!(env.home(), None, "the text view is std::env::var(..).ok()");
    assert_eq!(env.var("HOME"), None);
    assert_eq!(env.var_os("HOME"), Some(raw));
    assert!(env.is_set("HOME"), "set, as var_os(..).is_some()");
}

#[test]
fn the_compose_locale_follows_the_precedence_of_setlocale() {
    let locale = |pairs: &[(&str, &str)]| {
        let mut env = MapEnv::new();
        for (name, value) in pairs {
            env.set(name, value);
        }
        PlatformEnv::from_lookup(&env).compose_locale().to_string()
    };
    assert_eq!(locale(&[]), "C", "an unset environment has no compose file");
    assert_eq!(locale(&[("LANG", "de_DE.UTF-8")]), "de_DE.UTF-8");
    assert_eq!(
        locale(&[("LANG", "de_DE.UTF-8"), ("LC_CTYPE", "fr_FR.UTF-8")]),
        "fr_FR.UTF-8",
        "LC_CTYPE before LANG"
    );
    assert_eq!(
        locale(&[
            ("LANG", "de_DE.UTF-8"),
            ("LC_CTYPE", "fr_FR.UTF-8"),
            ("LC_ALL", "it_IT.UTF-8"),
        ]),
        "it_IT.UTF-8",
        "LC_ALL before both"
    );
    assert_eq!(
        locale(&[("LC_ALL", ""), ("LANG", "es_ES.UTF-8")]),
        "es_ES.UTF-8",
        "an empty LC_ALL is skipped, as setlocale does"
    );
}

#[test]
fn an_x11_session_needs_a_display_and_no_sign_of_wayland() {
    let session = |pairs: &[(&str, &str)]| {
        let mut env = MapEnv::new();
        for (name, value) in pairs {
            env.set(name, value);
        }
        PlatformEnv::from_lookup(&env).is_x11_session()
    };
    assert!(session(&[("DISPLAY", ":0")]));
    assert!(session(&[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "x11")]));
    assert!(!session(&[]), "no display at all");
    assert!(
        !session(&[("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0")]),
        "a Wayland socket is a Wayland session (XWayland sets DISPLAY too)"
    );
    assert!(!session(&[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "wayland")]));
    assert!(session(&[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "tty")]));
}
