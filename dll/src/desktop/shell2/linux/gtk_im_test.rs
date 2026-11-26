//! libgtk-3 is handed the caret rectangle of a focused text field (X11 with no XIM, Wayland after text-input v3). It
//! pulls in GDK, Pango, Cairo, GIO and about a hundred more libraries, so a window must not load it when it is
//! created (the startup timeline measured that load on every start): the first caret position loads it.
//!
//! A window cannot be created in a unit test (it needs a display server), so the sources say it: only the loader
//! dlopens libgtk-3, and each shell calls the loader from exactly one place, the initialiser of the once-cell that
//! holds the context.

const LOADER: &str = include_str!("gtk_im.rs");
const X11: &str = include_str!("x11/mod.rs");
const WAYLAND: &str = include_str!("wayland/mod.rs");

#[test]
fn only_the_loader_dlopens_libgtk() {
    assert_eq!(LOADER.matches("Gtk3Im::new()").count(), 1, "the loader is the one that dlopens it");
    assert_eq!(
        X11.matches("Gtk3Im::new()").count(),
        0,
        "the X11 window loaded libgtk-3 when it was created, on every start"
    );
    assert_eq!(
        WAYLAND.matches("Gtk3Im::new()").count(),
        0,
        "the Wayland window loaded libgtk-3 when it was created, on every start"
    );
}

#[test]
fn each_shell_loads_it_on_first_use_only() {
    for (shell, source) in [("X11", X11), ("Wayland", WAYLAND)] {
        let calls: Vec<usize> = source.match_indices("gtk_im::load(").map(|(at, _)| at).collect();
        assert_eq!(calls.len(), 1, "{shell}: one call of the loader");
        let before = &source[calls[0].saturating_sub(60)..calls[0]];
        assert!(
            before.contains("get_or_init(|| "),
            "{shell}: the call is the initialiser of the once-cell, not a line of the window's creation: {before:?}"
        );
    }
}
