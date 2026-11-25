use azul_css::env::MapEnv;

use super::*;

fn session(pairs: &[(&str, &str)]) -> PlatformEnv {
    let mut env = MapEnv::new();
    for (name, value) in pairs {
        env.set(name, value);
    }
    PlatformEnv::from_lookup(&env)
}

fn backend(window: Option<&str>, backend: Option<&str>, env: &PlatformEnv) -> Option<BackendType> {
    LinuxWindow::select_backend(window, backend, env).ok()
}

#[test]
fn the_session_decides_when_nothing_is_forced() {
    let wayland = session(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]);
    assert_eq!(backend(None, None, &wayland), Some(BackendType::Wayland), "XWayland sets both");
    let x11 = session(&[("DISPLAY", ":0")]);
    assert_eq!(backend(None, None, &x11), Some(BackendType::X11));
    assert!(
        LinuxWindow::select_backend(None, None, &PlatformEnv::default()).is_err(),
        "no display server at all"
    );
}

#[test]
fn the_window_choice_outranks_the_session_and_the_legacy_backend_choice() {
    let wayland = session(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]);
    assert_eq!(backend(Some("x11"), None, &wayland), Some(BackendType::X11));
    assert_eq!(backend(Some("X11"), None, &wayland), Some(BackendType::X11), "any case");
    let x11 = session(&[("DISPLAY", ":0")]);
    assert_eq!(backend(Some("wayland"), None, &x11), Some(BackendType::Wayland));
    // AZ_WINDOW wins over the legacy AZ_BACKEND=x11|wayland
    assert_eq!(backend(Some("x11"), Some("wayland"), &wayland), Some(BackendType::X11));
}

#[test]
fn the_legacy_backend_choice_counts_only_for_x11_and_wayland() {
    let wayland = session(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]);
    assert_eq!(backend(None, Some("x11"), &wayland), Some(BackendType::X11));
    assert_eq!(backend(None, Some("Wayland"), &session(&[("DISPLAY", ":0")])), Some(BackendType::Wayland));
    // the render values of AZ_BACKEND say nothing about the windowing system
    for render in ["cpu", "gpu", "auto", "headless"] {
        assert_eq!(backend(None, Some(render), &wayland), Some(BackendType::Wayland), "{render}");
    }
}

#[test]
fn auto_and_an_unknown_word_fall_through_to_the_session() {
    let x11 = session(&[("DISPLAY", ":0")]);
    assert_eq!(backend(Some("auto"), None, &x11), Some(BackendType::X11));
    assert_eq!(backend(Some("mir"), None, &x11), Some(BackendType::X11), "invalid: auto-detect");
}
