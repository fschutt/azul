//! The style discovery reads the platform lookups its app took at its start (`PlatformEnv`),
//! never the process: every test below hands it a `MapEnv`'s snapshot and a directory of its own.

use std::path::{Path, PathBuf};

use azul_css::env::MapEnv;

use super::*;

fn env_with(pairs: &[(&str, &str)]) -> PlatformEnv {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    PlatformEnv::from_lookup(&env)
}

/// A fresh directory below the temp directory, named for the test.
fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-style-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_desktop_is_detected_from_the_session_the_app_read() {
    let desktop = |pairs: &[(&str, &str)]| azul_css::system::detect_linux_desktop_env(&env_with(pairs));
    assert_eq!(desktop(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")]), DesktopEnvironment::Gnome);
    assert_eq!(desktop(&[("XDG_CURRENT_DESKTOP", "KDE")]), DesktopEnvironment::Kde);
    assert_eq!(desktop(&[("DESKTOP_SESSION", "plasma")]), DesktopEnvironment::Kde);
    assert_eq!(
        desktop(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc")]),
        DesktopEnvironment::Other("Hyprland".into())
    );
    assert_eq!(desktop(&[]), DesktopEnvironment::Other("Unknown".into()));
}

#[test]
fn the_gtk_settings_come_from_the_home_the_app_read_first_file_winning() {
    let home = fresh_dir("gtk");
    write(
        &home.join(".config/gtk-4.0/settings.ini"),
        "[Settings]\ngtk-theme-name=Four\ngtk-font-name=Cantarell 11\n",
    );
    write(
        &home.join(".config/gtk-3.0/settings.ini"),
        "[Settings]\ngtk-theme-name=Three\ngtk-icon-theme-name=Papirus\n",
    );
    let ini = gtk_settings_ini(&env_with(&[("HOME", text(&home))]));
    assert_eq!(ini.get("gtk-theme-name").map(String::as_str), Some("Four"));
    assert_eq!(ini.get("gtk-font-name").map(String::as_str), Some("Cantarell 11"));
    assert_eq!(ini.get("gtk-icon-theme-name").map(String::as_str), Some("Papirus"));

    // a file GTK_CONFIG_HOME names is read before both
    let custom = home.join("custom.ini");
    write(&custom, "gtk-theme-name=Custom\n");
    let ini = gtk_settings_ini(&env_with(&[("HOME", text(&home)), ("GTK_CONFIG_HOME", text(&custom))]));
    assert_eq!(ini.get("gtk-theme-name").map(String::as_str), Some("Custom"));

    // no home, no settings
    assert!(gtk_settings_ini(&PlatformEnv::default()).is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_kde_colour_sources_follow_the_home_and_the_config_dirs_in_precedence_order() {
    let home = fresh_dir("kde-home");
    let system = fresh_dir("kde-system");
    write(&home.join(".config/kcminputrc"), "[Mouse]\ncursorTheme=Bibata\n");
    write(&home.join(".config/kdeglobals"), "[General]\nColorScheme=Breeze Dark\n");
    write(
        &home.join(".local/share/color-schemes/BreezeDark.colors"),
        "[Colors:Window]\nBackgroundNormal=1,2,3\n",
    );
    write(&system.join("kdeglobals"), "[KDE]\nwidgetStyle=breeze\n");

    let env = env_with(&[("HOME", text(&home)), ("XDG_CONFIG_DIRS", text(&system))]);
    let sources = kde_color_sources(&env);
    assert_eq!(sources.len(), 4, "kcminputrc, kdeglobals, the scheme, the system defaults");
    assert_eq!(sources[0].get("Mouse", "cursorTheme"), Some("Bibata"));
    assert_eq!(sources[1].get("General", "ColorScheme"), Some("Breeze Dark"));
    assert_eq!(
        sources[2].color("Colors:Window", "BackgroundNormal"),
        Some(ColorU::new_rgb(1, 2, 3))
    );
    assert_eq!(sources[3].get("KDE", "widgetStyle"), Some("breeze"));

    // several system directories are searched in order, an empty entry is skipped
    let joined = format!("::{}", text(&system));
    let env = env_with(&[("HOME", text(&home)), ("XDG_CONFIG_DIRS", &joined)]);
    assert_eq!(kde_color_sources(&env).len(), 4);

    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&system);
}

#[test]
fn the_language_is_the_first_of_language_lang_and_lc_all_that_names_one() {
    let language = |pairs: &[(&str, &str)]| detect_language_linux(&env_with(pairs)).as_str().to_string();
    assert_eq!(language(&[]), "en-US");
    assert_eq!(language(&[("LANG", "de_DE.UTF-8")]), "de-DE");
    assert_eq!(language(&[("LANGUAGE", "fr:de"), ("LANG", "de_DE.UTF-8")]), "fr");
    assert_eq!(language(&[("LANG", "C"), ("LC_ALL", "pt_BR.UTF-8")]), "pt-BR");
    assert_eq!(language(&[("LANG", "POSIX")]), "en-US");
}

#[test]
fn the_cursor_comes_from_the_carried_xcursor_values_when_nothing_else_named_one() {
    let mut style = SystemStyle::default();
    style.linux.cursor_theme = OptionString::None;
    style.linux.cursor_size = 0;
    apply_env_cursor_fallbacks(&mut style, &env_with(&[("XCURSOR_THEME", "Adwaita"), ("XCURSOR_SIZE", "32")]));
    assert_eq!(style.linux.cursor_theme.as_option().map(|t| t.as_str()), Some("Adwaita"));
    assert_eq!(style.linux.cursor_size, 32);

    // what the desktop's own store answered is not overridden
    apply_env_cursor_fallbacks(&mut style, &env_with(&[("XCURSOR_THEME", "Other"), ("XCURSOR_SIZE", "48")]));
    assert_eq!(style.linux.cursor_theme.as_option().map(|t| t.as_str()), Some("Adwaita"));
    assert_eq!(style.linux.cursor_size, 32);

    // a size that is not a number is no size
    let mut style = SystemStyle::default();
    style.linux.cursor_size = 0;
    apply_env_cursor_fallbacks(&mut style, &env_with(&[("XCURSOR_SIZE", "big")]));
    assert_eq!(style.linux.cursor_size, 0);
}

#[test]
fn the_portal_needs_a_session_bus_address_with_a_unix_path() {
    let none = query_xdg_portal_inner(&PlatformEnv::default()).unwrap_err();
    assert!(none.contains("DBUS_SESSION_BUS_ADDRESS"), "{none}");
    let tcp = query_xdg_portal_inner(&env_with(&[("DBUS_SESSION_BUS_ADDRESS", "tcp:host=h,port=1")]))
        .unwrap_err();
    assert!(tcp.contains("no unix:path="), "{tcp}");
}

#[test]
fn the_theme_watcher_connects_to_the_bus_the_app_read_and_to_nothing_else() {
    assert_eq!(watch_portal_color_scheme(None), None, "no bus address, no watch");
    assert_eq!(watch_portal_color_scheme(Some("tcp:host=h,port=1")), None);
    assert_eq!(
        watch_portal_color_scheme(Some("unix:path=/nonexistent/azul-test-bus")),
        None,
        "a bus nobody listens on"
    );
}

// ── The kernel version comes from the kernel's release text, not from a `uname` process ────────

#[test]
fn a_kernel_release_text_names_the_os_version() {
    let linux = |id: u32| OsVersion::new(OsFamily::Linux, id);
    assert_eq!(linux_version_of("6.5.0-44-generic"), linux(6005));
    assert_eq!(linux_version_of("6.5.0-44-generic\n"), linux(6005), "the file ends in a newline");
    assert_eq!(linux_version_of("5.15.0-1070-azure"), linux(5015));
    assert_eq!(linux_version_of("6.8"), linux(6008));
    assert_eq!(linux_version_of("6.8-rc1"), linux(6008), "a release candidate has the minor of its release");
    assert_eq!(linux_version_of("4.19.317"), linux(4019));
}

#[test]
fn text_that_is_no_kernel_release_is_an_unknown_version() {
    for text in ["", "\n", "garbage", "6", "x.5", "6.x", "-1.2"] {
        assert_eq!(linux_version_of(text), OsVersion::unknown(), "{text:?}");
    }
}

#[test]
fn the_discovery_starts_no_uname_process() {
    // A start before the first frame is not the place for a process whose answer the kernel keeps in a file.
    let source = include_str!("system_style.rs");
    let start = source.find("fn detect_linux_version(").expect("the function is there");
    let body = &source[start..start + source[start..].find("\n}\n").expect("it ends")];
    assert!(body.contains("/proc/sys/kernel/osrelease"), "it reads the kernel's file");
    assert!(!body.contains("uname"), "and spawns nothing: {body}");
}
