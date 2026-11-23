use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn an_empty_environment_is_the_default() {
    assert_eq!(PlatformEnv::from_lookup(&NoEnv), PlatformEnv::default());
    assert_eq!(PlatformEnv::from_lookup(&MapEnv::new()), PlatformEnv::default());
    assert_eq!(PlatformEnv::default().home_dir(), None);
}

#[test]
fn the_variables_are_read_by_their_names() {
    let env = MapEnv::new()
        .with("HOME", "/home/u")
        .with("USERPROFILE", "C:\\Users\\u")
        .with("XDG_CONFIG_HOME", "/cfg")
        .with("XDG_DATA_HOME", "/data")
        .with("APPDATA", "C:\\Roaming")
        .with("LOCALAPPDATA", "C:\\Local")
        .with("SNAP", "")
        .with("WAYLAND_DISPLAY", "wayland-0")
        .with("DISPLAY", ":0");
    let platform = PlatformEnv::from_lookup(&env);
    assert_eq!(platform.home.as_deref(), Some(OsStr::new("/home/u")));
    assert_eq!(
        platform.user_profile.as_deref(),
        Some(OsStr::new("C:\\Users\\u"))
    );
    assert_eq!(platform.xdg_config_home.as_deref(), Some(OsStr::new("/cfg")));
    assert_eq!(platform.xdg_data_home.as_deref(), Some(OsStr::new("/data")));
    assert_eq!(platform.app_data.as_deref(), Some(OsStr::new("C:\\Roaming")));
    assert_eq!(
        platform.local_app_data.as_deref(),
        Some(OsStr::new("C:\\Local"))
    );
    assert!(platform.snap, "an empty value is set");
    assert!(platform.wayland_display);
    assert!(platform.display);
}

#[test]
fn the_home_folder_is_home_else_the_user_profile() {
    let both = PlatformEnv::from_lookup(&MapEnv::new().with("HOME", "/h").with("USERPROFILE", "/p"));
    assert_eq!(both.home_dir(), Some(OsStr::new("/h")));
    let profile = PlatformEnv::from_lookup(&MapEnv::new().with("USERPROFILE", "/p"));
    assert_eq!(profile.home_dir(), Some(OsStr::new("/p")));
    // a HOME that is set but empty stays what it is: the user of it decides
    let empty = PlatformEnv::from_lookup(&MapEnv::new().with("HOME", "").with("USERPROFILE", "/p"));
    assert_eq!(empty.home_dir(), Some(OsStr::new("")));
}

#[cfg(all(unix, feature = "std"))]
#[test]
fn a_path_that_is_not_text_keeps_its_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let raw = OsString::from_vec(vec![b'/', b'h', 0xff, b'x']);
    let platform = PlatformEnv::from_lookup(&MapEnv::new().with_os("HOME", raw.clone()));
    assert_eq!(platform.home, Some(raw));
}
