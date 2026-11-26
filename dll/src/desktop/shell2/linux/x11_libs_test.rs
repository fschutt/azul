//! The libraries an X11 window loads before it exists. libEGL is optional on every host: a machine without it (a
//! container, a VNC desktop, a minimal install; the first runner of the startup measurement) opens its windows on
//! the CPU, and a window that asks for the CPU renderer does not load the library at all. A window cannot be made in
//! a unit test (it needs an X server), so the source says it.

const X11: &str = include_str!("x11/mod.rs");

#[test]
fn a_machine_without_libegl_still_opens_its_window() {
    assert!(
        !X11.contains("Failed to load libEGL"),
        "a missing libEGL was a reason not to open the window, even for the CPU renderer"
    );
    assert!(
        X11.contains("let egl = if force_cpu {"),
        "the library is loaded when the window may use GL, and its absence is an Option"
    );
}

#[test]
fn a_cpu_window_decides_its_renderer_before_it_loads_libegl() {
    let decided = X11.find("let force_cpu = matches!(").expect("the renderer is decided");
    let loaded = X11.find("Egl::new()").expect("libEGL is loaded somewhere");
    assert!(decided < loaded, "`force_cpu` must be known when libEGL is (not) loaded");
    assert_eq!(X11.matches("let force_cpu = matches!(").count(), 1, "decided once");
}
