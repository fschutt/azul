//! libgtk-3 is the fallback that positions an IME's candidate window when there is no XIM. It pulls in GDK, Pango,
//! Cairo, GIO and about a hundred more libraries, so a window must not load it when it is created (the startup
//! timeline measured that load on every start): the first position that finds no XIM loads it.
//!
//! The window cannot be created in a unit test (it needs an X server), so the source says it: the loader is called
//! from exactly one place, the `get_or_init` of the field that holds the context.

const SOURCE: &str = include_str!("mod.rs");

/// The text of the first function whose signature starts with `signature`, up to its closing brace in column 0.
fn function_text(signature: &str) -> &'static str {
    let start = SOURCE
        .find(signature)
        .unwrap_or_else(|| panic!("`{signature}` is not in the X11 shell"));
    let rest = &SOURCE[start..];
    let end = rest.find("\n}\n").expect("the function ends at a closing brace in column 0");
    &rest[..end]
}

#[test]
fn the_gtk_input_method_is_loaded_by_one_function() {
    let loader = function_text("fn load_gtk_im(");
    assert!(
        loader.contains("Gtk3Im::new()"),
        "the loader is the one that dlopens libgtk-3"
    );
    let loads = SOURCE.matches("Gtk3Im::new()").count();
    assert_eq!(loads, 1, "no other place loads libgtk-3 (a window creation did, on every start)");
}

#[test]
fn the_loader_runs_on_first_use_only() {
    let calls: Vec<usize> = SOURCE.match_indices("load_gtk_im(").map(|(at, _)| at).collect();
    assert_eq!(calls.len(), 2, "its definition and one call");
    let call = calls[1];
    let before = &SOURCE[call.saturating_sub(40)..call];
    assert!(
        before.contains("get_or_init(|| "),
        "the one call is the initialiser of the once-cell, not a line of the window's creation: {before:?}"
    );
}
