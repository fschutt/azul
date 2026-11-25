use azul_css::env::MapEnv;

use super::*;

fn directories(pairs: &[(&str, &str)]) -> (String, String) {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    let (data, cache) = persistent_directories_of("notes", &PlatformEnv::from_lookup(&env));
    (
        data.into_string().unwrap(),
        cache.into_string().unwrap(),
    )
}

#[test]
fn the_web_data_lives_under_the_xdg_directories_of_the_app_read_at_its_start() {
    let (data, cache) = directories(&[
        ("HOME", "/home/u"),
        ("XDG_DATA_HOME", "/xdg/data"),
        ("XDG_CACHE_HOME", "/xdg/cache"),
    ]);
    assert_eq!(data, "/xdg/data/notes/webview");
    assert_eq!(cache, "/xdg/cache/notes/webview");
}

#[test]
fn without_xdg_directories_the_home_decides() {
    let (data, cache) = directories(&[("HOME", "/home/u")]);
    assert_eq!(data, "/home/u/.local/share/notes/webview");
    assert_eq!(cache, "/home/u/.cache/notes/webview");
    // an empty variable names nothing
    let (data, cache) = directories(&[
        ("HOME", "/home/u"),
        ("XDG_DATA_HOME", ""),
        ("XDG_CACHE_HOME", ""),
    ]);
    assert_eq!(data, "/home/u/.local/share/notes/webview");
    assert_eq!(cache, "/home/u/.cache/notes/webview");
}

#[test]
fn without_a_home_the_directories_fall_back_to_tmp() {
    let (data, cache) = directories(&[]);
    assert_eq!(data, "/tmp/.local/share/notes/webview");
    assert_eq!(cache, "/tmp/.cache/notes/webview");
}
