use azul_css::env::MapEnv;

use super::*;

/// The platform lookups of a session with the given variables.
fn platform(pairs: &[(&str, &str)]) -> PlatformEnv {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    PlatformEnv::from_lookup(&env)
}

/// The Linux switches with the given variables.
fn linux(pairs: &[(&str, &str)]) -> LinuxSettings {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    LinuxSettings::from_lookup(&env)
}

const BUS: (&str, &str) = ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus");

#[test]
fn a_gnome_session_with_a_bus_gets_the_gnome_menus() {
    let env = platform(&[("XDG_CURRENT_DESKTOP", "GNOME"), BUS]);
    assert!(should_use_gnome_menus(&LinuxSettings::default(), &env));
    // the spellings desktops use for a GNOME-based session
    for desktop in ["ubuntu:GNOME", "GNOME-Flashback:GNOME", "gnome"] {
        let env = platform(&[("XDG_CURRENT_DESKTOP", desktop), BUS]);
        assert!(should_use_gnome_menus(&LinuxSettings::default(), &env), "{desktop}");
    }
}

#[test]
fn the_disable_flag_wins_over_a_gnome_session() {
    let env = platform(&[("XDG_CURRENT_DESKTOP", "GNOME"), BUS]);
    assert!(!should_use_gnome_menus(
        &linux(&[("AZ_DISABLE_GNOME_MENUS", "1")]),
        &env
    ));
    // only the value 1 disables
    assert!(should_use_gnome_menus(
        &linux(&[("AZ_DISABLE_GNOME_MENUS", "0")]),
        &env
    ));
}

#[test]
fn another_desktop_or_none_gets_client_side_menus() {
    for desktop in ["KDE", "XFCE", "sway", ""] {
        let env = platform(&[("XDG_CURRENT_DESKTOP", desktop), BUS]);
        assert!(!should_use_gnome_menus(&LinuxSettings::default(), &env), "{desktop:?}");
    }
    assert!(
        !should_use_gnome_menus(&LinuxSettings::default(), &platform(&[BUS])),
        "an unset desktop"
    );
}

#[test]
fn a_gnome_session_without_a_session_bus_gets_client_side_menus() {
    let env = platform(&[("XDG_CURRENT_DESKTOP", "GNOME")]);
    assert!(!should_use_gnome_menus(&LinuxSettings::default(), &env));
}

#[test]
fn the_decision_reads_the_carried_values_and_not_the_process() {
    // whatever this process has in its environment, the empty one decides "no GNOME"
    assert!(!should_use_gnome_menus(
        &LinuxSettings::default(),
        &PlatformEnv::default()
    ));
}

#[test]
fn the_trace_is_the_switch_the_app_read() {
    assert_eq!(MenuTrace::new(false), MenuTrace::OFF);
    assert_eq!(MenuTrace::default(), MenuTrace::OFF);
    assert_ne!(MenuTrace::new(true), MenuTrace::OFF);
    assert_eq!(
        MenuTrace::new(linux(&[("AZ_GNOME_MENU_DEBUG", "1")]).gnome_menu_debug),
        MenuTrace::new(true)
    );
    // logging with the trace off is a no-op that must not panic
    MenuTrace::OFF.log("nothing");
}
