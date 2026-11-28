//! The Java half of `WindowFlags::system_bars` (the status and navigation bar modes, the icon colour, drawing into the cutout area)
//! and the shell's call into it, pinned in source (ledger: the TABS agent's contract, item 5).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_system_bars_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn java_code(rel: &str) -> String {
    read(rel).lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}`"));
    let open = start + src[start..].find('{').unwrap();
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{signature}`");
}

/// A hidden bar is shown again for a moment by a swipe (transient) or until the app hides it (sticky), the window draws into the cutout
/// area with `ALWAYS` from Android 11 and `SHORT_EDGES` before, edge to edge, only when asked; fixed icon colours stand the shell's own
/// pick down.
///
/// Mutation: `layoutInDisplayCutoutMode` set whatever `drawIntoCutout` says (the status of `extend_into_safe_area` alone changed the cutout).
#[test]
fn the_activity_applies_the_bars_and_the_cutout_the_window_asks_for() {
    let java = java_code("scripts/android/AzulActivity.java");
    let apply = body(&java, "public static void applySystemBars(");
    assert!(apply.contains("if (drawIntoCutout && android.os.Build.VERSION.SDK_INT >= 28)"), "the cutout mode only when asked, from Android 9");
    assert!(apply.contains("LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS") && apply.contains("LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES"));
    assert!(apply.contains("window.setDecorFitsSystemWindows(false)"), "edge to edge");
    assert!(apply.contains("BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE") && apply.contains("BEHAVIOR_DEFAULT"), "transient and sticky");
    assert!(apply.contains("controller.hide(type)") || java.contains("controller.hide(type);"), "a hidden bar is hidden by the controller");
    assert!(apply.contains("if (icons != 0)"), "Auto leaves the colour of the icons to the shell");
    assert!(apply.contains("runOnUiThread"));
}
