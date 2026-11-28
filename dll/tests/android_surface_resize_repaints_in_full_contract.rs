//! A surface of another size is repainted in full by the Android shell, pinned in source.
//!
//! The CPU backend keeps the last frame and, when little changed, repaints only what did. Across a change of the
//! window's size it keeps the corner the two sizes share and repaints what the relayout changed plus the strip a side gained
//! (`CpuBackend::render_frame`, "a mixed resize"), and the rotation of a phone to landscape is exactly such a resize (wider,
//! shorter). QA's emulator run 45797870060 (AzWidgets, 2400 x 1080 after the rotation): the page's white ended at x = 1080 px,
//! the width the window had in portrait, and what lay beyond was the canvas colour, while a render of the SAME display list
//! from scratch (`take_screenshot`) was right and the shell's three presents after the rotation each wrote all 1080 rows
//! ("present full - rows 0..1080 of 1080"). The present was faithful: the picture it was given was wrong.
//!
//! The shell is compiled for `target_os = "android"` only, so what it does with a new size is read here:
//!
//! * the window's size is written in ONE place, and that place asks for the full repaint when the size changed;
//! * the three events that carry a surface size (`InitWindow`, `WindowResized`, `ConfigChanged`) write it there.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_surface_resize_repaints_in_full_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

/// The code of the Android shell: comments dropped, whitespace removed (rustfmt breaks the calls over lines).
fn shell() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dll/src/desktop/shell2/android/mod.rs");
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The text of `code` from the first `needle` to its matching `}` (the body of an item or an arm). A needle that ends in
/// `{` names the brace itself (the arm `MainEvent::InitWindow{..}=>{` has an earlier one in its pattern).
fn block<'a>(code: &'a str, needle: &str) -> &'a str {
    let start = code.find(needle).unwrap_or_else(|| panic!("no `{needle}` in the Android shell"));
    let open = if needle.ends_with('{') {
        start + needle.len() - 1
    } else {
        start + code[start..].find('{').expect("a block follows")
    };
    let mut depth = 0usize;
    for (i, c) in code[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &code[start..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{needle}`");
}

/// A frame at another size than the last one is painted in full.
///
/// Mutation: the size written with no question asked (the state of 1826eb5, where only a change of the scale asked for the
/// full repaint): the strip a rotation exposes is left to the incremental repaint.
#[test]
fn a_size_that_changed_asks_for_the_full_repaint() {
    let code = shell();
    let helper = block(&code, "fnapply_surface_size(");
    assert!(
        helper.contains("now.width.to_bits()!=logical_w.to_bits()||now.height.to_bits()!=logical_h.to_bits()"),
        "the new size is compared with the window's current one, in both dimensions: {helper}"
    );
    let ask = helper.find("window.cpu_backend.force_full_repaint=true;").expect("a changed size asks for the full repaint");
    let compare = helper.find("now.width.to_bits()").unwrap();
    let write = helper.find("ws.size.dimensions.width=logical_w;").expect("the helper writes the size");
    assert!(compare < ask && ask < write, "the question is asked of the size the window HAS, before the new one is written");
}

/// There is no second place that writes the window's size: a surface event that wrote it directly would skip the question.
///
/// Mutation: `WindowResized` writing `ws.size.dimensions` itself (the shape before this contract existed: three copies).
#[test]
fn the_surface_events_write_the_size_through_it() {
    let code = shell();
    assert_eq!(
        code.matches("ws.size.dimensions.width=").count(),
        1,
        "the window's width is written in `apply_surface_size` and nowhere else in the shell"
    );
    assert_eq!(
        code.matches("ws.size.dimensions.height=").count(),
        1,
        "the window's height is written in `apply_surface_size` and nowhere else in the shell"
    );
    for (arm, what) in [
        ("MainEvent::InitWindow{..}=>{", "the surface a rotation or a return from the background made"),
        ("MainEvent::WindowResized{..}=>{", "the resize the system reports"),
        ("MainEvent::ConfigChanged{..}=>{", "the size that came with a new density"),
    ] {
        let body = block(&code, arm);
        assert!(body.contains("apply_surface_size(window,"), "{arm} ({what}) writes the size through `apply_surface_size`");
    }
}
