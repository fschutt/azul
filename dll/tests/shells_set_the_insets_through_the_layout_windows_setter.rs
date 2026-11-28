//! The Android and the iOS shell hand the system's insets over through `LayoutWindow::set_safe_area_insets`, pinned in source.
//!
//! The setter is what tells the scroll manager where the on-screen keyboard's top edge is and scrolls the focused field above a
//! keyboard that comes up: a shell that wrote `lw.safe_area_insets` itself would leave the field of a scrolling page under the
//! keyboard (a phone's alarm label, a note, a mail's body), as every typing app did. The shells are compiled for their own
//! targets only, so what they do with an inset is read here.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/shells_set_the_insets_through_the_layout_windows_setter.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

/// The code of a source file: comments dropped, whitespace removed (rustfmt breaks the calls over lines).
fn code(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Mutation: a shell writing the field itself (the state of 3d343de6bc): the keyboard's top edge is never told, nothing scrolls.
#[test]
fn no_shell_writes_the_safe_area_insets_of_the_layout_window_itself() {
    for (file, writes) in [
        ("dll/src/desktop/shell2/android/mod.rs", 1),
        ("dll/src/desktop/shell2/ios/mod.rs", 2),
    ] {
        let shell = code(file);
        assert!(!shell.contains("lw.safe_area_insets="), "{file} assigns `lw.safe_area_insets` itself");
        assert!(!shell.contains("lw.safe_area_insets.keyboard="), "{file} assigns the keyboard's inset itself");
        assert_eq!(shell.matches("lw.set_safe_area_insets(").count(), writes, "{file} goes through the setter at each of its {writes} site(s)");
    }
}

/// The setter is the one writer in the layout crate too, and it is where the keyboard reaches the scroll manager.
#[test]
fn the_setter_tells_the_scroll_manager_and_reveals_the_focused_node() {
    let layout = code("layout/src/window.rs");
    let at = layout.find("pubfnset_safe_area_insets(").expect("the setter");
    let body = &layout[at..at + 1800];
    assert!(body.contains("self.safe_area_insets=insets;"), "it stores the insets");
    assert!(body.contains("self.scroll_manager.set_covered_below("), "it tells the scroll manager where the keyboard's top edge is");
    assert!(body.contains("self.focus_manager.get_focused_node()"), "the focused node is the one revealed");
    assert!(body.contains("scroll_node_into_view("), "through the reveal the focus path uses");
    assert!(body.contains("ifkeyboard<=0.0"), "a keyboard that goes away asks for no scroll");
}
